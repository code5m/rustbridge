//! Run with `rustc -O scripts/directory-sort-bench.rs -o /tmp/rustbridge-sort-bench`
//! followed by `/tmp/rustbridge-sort-bench /path/to/directory`.
use std::{fs, hint::black_box, time::Instant};

#[derive(Clone)]
struct Entry {
    name: String,
    directory: bool,
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("directory argument required");
    let entries: Vec<Entry> = fs::read_dir(path)
        .expect("read directory")
        .map(|entry| {
            let entry = entry.expect("read entry");
            Entry {
                name: entry.file_name().to_string_lossy().into_owned(),
                directory: entry.file_type().expect("read entry type").is_dir(),
            }
        })
        .collect();
    let mut old = Vec::new();
    let mut cached = Vec::new();
    for iteration in 0..52 {
        let mut before = entries.clone();
        let mut after = entries.clone();
        let start = Instant::now();
        before.sort_by(|a, b| {
            b.directory
                .cmp(&a.directory)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        let old_time = start.elapsed().as_micros();
        let start = Instant::now();
        after.sort_by_cached_key(|entry| (!entry.directory, entry.name.to_lowercase()));
        let cached_time = start.elapsed().as_micros();
        assert!(
            before
                .iter()
                .zip(&after)
                .all(|(a, b)| a.name == b.name && a.directory == b.directory)
        );
        black_box((&before, &after));
        if iteration >= 2 {
            old.push(old_time);
            cached.push(cached_time);
        }
    }
    old.sort_unstable();
    cached.sort_unstable();
    println!(
        "entries={} samples=50 old_us={} cached_us={}",
        entries.len(),
        old[25],
        cached[25]
    );
}
