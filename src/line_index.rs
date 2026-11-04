//! Minimal physical-source edit spans and incremental line-offset maintenance.
//! Immutable source texts remain strings; this vector is only an index to real bytes.
use std::ops::Range;

pub fn minimal_diff<'a>(before: &str, after: &'a str) -> (Range<usize>, &'a str) {
    let mut begin = before
        .bytes()
        .zip(after.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !before.is_char_boundary(begin) || !after.is_char_boundary(begin) {
        begin -= 1;
    }
    let mut old_end = before.len();
    let mut new_end = after.len();
    while old_end > begin
        && new_end > begin
        && before.as_bytes()[old_end - 1] == after.as_bytes()[new_end - 1]
    {
        old_end -= 1;
        new_end -= 1;
    }
    while !before.is_char_boundary(old_end) || !after.is_char_boundary(new_end) {
        old_end += 1;
        new_end += 1;
    }
    (begin..old_end, &after[begin..new_end])
}
pub fn apply_line_patch(starts: &mut Vec<usize>, physical_range: Range<usize>, inserted: &str) {
    let left = starts.partition_point(|offset| *offset <= physical_range.start);
    let right = starts.partition_point(|offset| *offset <= physical_range.end);
    starts.drain(left..right);
    let delta = inserted.len() as isize - (physical_range.end - physical_range.start) as isize;
    for offset in &mut starts[left..] {
        *offset = offset
            .checked_add_signed(delta)
            .expect("line index valid for source");
    }
    let inserted_lines = inserted
        .match_indices('\n')
        .map(|(index, _)| physical_range.start + index + 1)
        .collect::<Vec<_>>();
    starts.splice(left..left, inserted_lines);
}
#[cfg(test)]
mod tests {
    use super::*;
    fn full(s: &str) -> Vec<usize> {
        let mut result = vec![0];
        for (index, _) in s.match_indices('\n') {
            result.push(index + 1);
        }
        result
    }
    #[test]
    fn unicode_mixed_eol_deletes_and_inserts_match_full_reindex() {
        let old = "a🦀\r\n中文\nb\rc\n";
        let new = "a🦀\n🥨中文\nother\n";
        let (range, insert) = minimal_diff(old, new);
        let mut line_starts = full(old);
        apply_line_patch(&mut line_starts, range, insert);
        assert_eq!(line_starts, full(new));
    }
    #[test]
    fn randomized_6000_incremental_edits_have_exact_offsets() {
        fn next(seed: &mut u64) -> u64 {
            *seed ^= *seed << 13;
            *seed ^= *seed >> 7;
            *seed ^= *seed << 17;
            *seed
        }
        let mut seed = 0x9aa55a9a77_u64;
        let pieces = ["a", "b", "\n", "\r", "\r\n", "中文", "🦀", "   ", "let"];
        let mut text = "struct 示例 {\n    data: String,\n}\n".to_owned();
        let mut lines = full(&text);
        for trial in 0..6000 {
            let boundaries = text
                .char_indices()
                .map(|(i, _)| i)
                .chain([text.len()])
                .collect::<Vec<_>>();
            let a = boundaries[(next(&mut seed) % boundaries.len() as u64) as usize];
            let b = boundaries[(next(&mut seed) % boundaries.len() as u64) as usize];
            let (start, end) = (a.min(b), a.max(b));
            let insert = pieces[(next(&mut seed) % pieces.len() as u64) as usize];
            let mut next_text = text.clone();
            next_text.replace_range(start..end, insert);
            let (range, change) = minimal_diff(&text, &next_text);
            apply_line_patch(&mut lines, range, change);
            assert_eq!(lines, full(&next_text), "random edit trial {trial}");
            text = next_text;
        }
    }
}
