//! Optional syntax-only project declaration lookup, not a Rust name/type resolver.
//! Runs off the egui thread and never executes project code.
use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    thread,
};
use tree_sitter::{Node, ParseOptions, Parser, Point, Tree};

#[derive(Clone, Debug)]
pub struct Declaration {
    pub path: PathBuf,
    pub name: String,
    pub kind: String,
    pub byte_start: usize,
    pub byte_end: usize,
    pub line: usize,
}
#[derive(Default)]
struct State {
    pending: Option<(u64, PathBuf, String)>,
    result: Option<(u64, Vec<Declaration>)>,
    closed: bool,
}
struct Shared {
    state: Mutex<State>,
    ready: Condvar,
    latest: AtomicU64,
}
pub struct SymbolSearch {
    shared: Arc<Shared>,
}
impl SymbolSearch {
    pub fn new() -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            ready: Condvar::new(),
            latest: AtomicU64::new(0),
        });
        let copy = Arc::clone(&shared);
        thread::spawn(move || {
            let mut parser = Parser::new();
            if parser
                .set_language(&tree_sitter_rust::LANGUAGE.into())
                .is_err()
            {
                return;
            }
            loop {
                let (rev, root, query) = {
                    let mut state = copy.state.lock().unwrap();
                    while state.pending.is_none() && !state.closed {
                        state = copy.ready.wait(state).unwrap();
                    }
                    if state.closed {
                        return;
                    }
                    state.pending.take().unwrap()
                };
                let cancelled = || copy.latest.load(Ordering::Acquire) != rev;
                let results = find_declarations(&mut parser, &root, &query, &cancelled);
                if cancelled() {
                    continue;
                }
                let mut state = copy.state.lock().unwrap();
                if !state.closed && !cancelled() {
                    state.result = Some((rev, results));
                }
            }
        });
        Self { shared }
    }
    pub fn submit(&self, rev: u64, root: PathBuf, query: String) {
        let mut state = self.shared.state.lock().unwrap();
        self.shared.latest.store(rev, Ordering::Release);
        state.pending = Some((rev, root, query));
        state.result = None;
        self.shared.ready.notify_one();
    }
    pub fn take(&self) -> Option<(u64, Vec<Declaration>)> {
        self.shared.state.lock().unwrap().result.take()
    }
    pub fn cancel(&self, revision: u64) {
        let mut state = self.shared.state.lock().unwrap();
        self.shared.latest.store(revision, Ordering::Release);
        state.pending = None;
        state.result = None;
    }
}
impl Drop for SymbolSearch {
    fn drop(&mut self) {
        let mut state = self.shared.state.lock().unwrap();
        self.shared.latest.fetch_add(1, Ordering::AcqRel);
        state.closed = true;
        state.pending = None;
        self.shared.ready.notify_one();
    }
}
impl Default for SymbolSearch {
    fn default() -> Self {
        Self::new()
    }
}

fn scan_node(
    node: Node<'_>,
    source: &str,
    path: &PathBuf,
    target: &str,
    hits: &mut Vec<Declaration>,
    cancelled: &impl Fn() -> bool,
) {
    if cancelled() || hits.len() >= 100 {
        return;
    }
    if matches!(
        node.kind(),
        "struct_item"
            | "enum_item"
            | "trait_item"
            | "function_item"
            | "const_item"
            | "type_item"
            | "mod_item"
            | "static_item"
            | "union_item"
    ) && let Some(name) = node.child_by_field_name("name")
        && name.utf8_text(source.as_bytes()).ok() == Some(target)
    {
        hits.push(Declaration {
            path: path.clone(),
            name: target.to_owned(),
            kind: node.kind().to_owned(),
            byte_start: name.start_byte(),
            byte_end: name.end_byte(),
            line: name.start_position().row,
        });
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if hits.len() >= 100 || cancelled() {
            break;
        }
        if child.is_named() {
            scan_node(child, source, path, target, hits, cancelled);
        }
    }
}

fn parse_source(parser: &mut Parser, source: &str, cancelled: &impl Fn() -> bool) -> Option<Tree> {
    if cancelled() {
        return None;
    }
    let mut progress = |_state: &tree_sitter::ParseState| cancelled();
    let options = ParseOptions::new().progress_callback(&mut progress);
    let mut input = |byte: usize, _point: Point| &source.as_bytes()[byte..];
    let tree = parser.parse_with_options(&mut input, None, Some(options));
    if tree.is_none() {
        parser.reset();
    }
    tree.filter(|_| !cancelled())
}

fn find_declarations(
    parser: &mut Parser,
    root: &Path,
    query: &str,
    cancelled: &impl Fn() -> bool,
) -> Vec<Declaration> {
    let mut hits = Vec::new();
    let mut queue = VecDeque::from([(root.to_path_buf(), 0u16)]);
    let mut visited = 0usize;
    while let Some((dir, depth)) = queue.pop_front() {
        if cancelled() || depth > 24 || visited >= 2000 || hits.len() >= 100 {
            break;
        }
        let Ok(iter) = fs::read_dir(dir) else {
            continue;
        };
        for entry in iter.flatten() {
            if cancelled() || visited >= 2000 || hits.len() >= 100 {
                break;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() {
                continue;
            }
            let name = entry.file_name();
            if matches!(name.to_str(), Some(".git" | "target" | "node_modules")) {
                continue;
            }
            let path = entry.path();
            if kind.is_dir() {
                queue.push_back((path, depth + 1));
            } else if kind.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
                visited += 1;
                if entry
                    .metadata()
                    .is_ok_and(|meta| meta.len() > 2 * 1024 * 1024)
                {
                    continue;
                }
                let Ok(source) = fs::read_to_string(&path) else {
                    continue;
                };
                let Some(tree) = parse_source(parser, &source, cancelled) else {
                    continue;
                };
                scan_node(
                    tree.root_node(),
                    &source,
                    &path,
                    query,
                    &mut hits,
                    cancelled,
                );
            }
        }
    }
    hits.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then_with(|| a.byte_start.cmp(&b.byte_start))
    });
    hits
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    #[test]
    fn cancelled_parse_can_be_reused_for_the_next_file() {
        use std::cell::Cell;
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let checks = Cell::new(0);
        let source = "fn sample() { let value = 123; }\n".repeat(20_000);
        let cancelled = || {
            checks.set(checks.get() + 1);
            checks.get() > 1
        };
        assert!(parse_source(&mut parser, &source, &cancelled).is_none());
        assert!(checks.get() > 1);
        let tree = parse_source(&mut parser, "fn next() {}", &|| false).unwrap();
        assert!(!tree.root_node().has_error());
        assert_eq!(tree.root_node().end_byte(), "fn next() {}".len());
    }
    #[test]
    fn cancel_and_drop_invalidate_old_work() {
        let worker = SymbolSearch::new();
        let shared = Arc::clone(&worker.shared);
        worker.submit(1, PathBuf::from("missing"), "Foo".into());
        worker.cancel(2);
        {
            let state = shared.state.lock().unwrap();
            assert_eq!(shared.latest.load(Ordering::Acquire), 2);
            assert!(state.pending.is_none());
            assert!(state.result.is_none());
        }
        worker.submit(3, PathBuf::from("missing"), "Bar".into());
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some((revision, _)) = worker.take() {
                assert_eq!(revision, 3);
                break;
            }
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(5));
        }
        drop(worker);
        assert_ne!(shared.latest.load(Ordering::Acquire), 3);
    }
    #[test]
    fn finds_only_syntax_declarations_across_files_not_identical_name_uses() {
        let root = std::env::temp_dir().join(format!("rustbridge-symbol-{}", std::process::id()));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/a.rs"), "struct Foo {}; fn same(){}").unwrap();
        fs::write(
            root.join("src/b.rs"),
            "fn f(){ let _ = Foo {}; } fn same() {}",
        )
        .unwrap();
        let worker = SymbolSearch::new();
        worker.submit(1, root.clone(), "Foo".into());
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some((rev, hits)) = worker.take() {
                assert_eq!(rev, 1);
                assert_eq!(hits.len(), 1);
                assert!(hits[0].path.ends_with("a.rs"));
                assert_eq!(hits[0].kind, "struct_item");
                break;
            }
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(5));
        }
        fs::remove_dir_all(root).unwrap();
    }
}
