//! Reversible newline adapter for an egui TextEdit.
//! Rust source remains byte-for-byte unchanged outside the user's edit.
//! The editing widget displays CRLF as LF and translates edited spans back
//! into the original file's physical newline convention.

/// Translate CRLF pairs for display. Original code stays in the Document.
pub fn for_display(original: &str) -> String {
    original.replace("\r\n", "\n")
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

fn nearest_line_ending_is_crlf(original: &str, physical: usize) -> bool {
    if let Some(relative) = original[physical..].find('\n') {
        let newline = physical + relative;
        return newline > 0 && original.as_bytes()[newline - 1] == b'\r';
    }
    if let Some(newline) = original[..physical].rfind('\n') {
        return newline > 0 && original.as_bytes()[newline - 1] == b'\r';
    }
    false
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
    let replacement = if nearest_line_ending_is_crlf(original, physical_start) {
        insertion.replace('\n', "\r\n")
    } else {
        insertion.to_owned()
    };
    let mut result =
        String::with_capacity(original.len() - (physical_end - physical_start) + replacement.len());
    result.push_str(&original[..physical_start]);
    result.push_str(&replacement);
    result.push_str(&original[physical_end..]);
    result
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
    fn plain_lf_files_are_unmodified_except_actual_edit() {
        let original = "a\nb\n";
        let before = for_display(original);
        assert_eq!(apply_display_edit(original, &before, "a\nBB\n"), "a\nBB\n");
    }
}
