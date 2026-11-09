//! Compile with `rustc -O scripts/directory-walk-bench.rs -o /tmp/rustbridge-walk-bench`.
//! Run with `old` or `new`, followed by an isolated test directory path.
use std::{collections::VecDeque, env, fs, io, path::PathBuf, time::Instant};

fn main() -> io::Result<()> {
    let mut args = env::args().skip(1);
    let mode = args.next().expect("old or new");
    let root = PathBuf::from(args.next().expect("directory"));
    let start = Instant::now();
    let mut count = 0usize;
    let mut peak = 0usize;
    if mode == "old" {
        let mut queue = VecDeque::from([(root, 0)]);
        while let Some((path, depth)) = queue.pop_front() {
            if depth > 12 {
                break;
            }
            for entry in fs::read_dir(path)?.flatten() {
                let kind = entry.file_type()?;
                if kind.is_dir() {
                    queue.push_back((entry.path(), depth + 1));
                    peak = peak.max(queue.len());
                } else if kind.is_file() && entry.path().extension().is_some_and(|s| s == "rs") {
                    count += 1;
                }
            }
        }
    } else if mode == "new" {
        let mut stack = vec![(fs::read_dir(root)?, 0)];
        peak = 1;
        while let Some((entries, depth)) = stack.last_mut() {
            let Some(entry) = entries.next() else {
                stack.pop();
                continue;
            };
            let depth = *depth;
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_dir() && depth < 12 {
                stack.push((fs::read_dir(entry.path())?, depth + 1));
                peak = peak.max(stack.len());
            } else if kind.is_file() && entry.path().extension().is_some_and(|s| s == "rs") {
                count += 1;
            }
        }
    } else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected old or new",
        ));
    }
    println!(
        "mode={mode} entries={count} pending_peak={peak} elapsed_us={}",
        start.elapsed().as_micros()
    );
    Ok(())
}
