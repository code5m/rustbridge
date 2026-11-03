# RustBridge 🦀☕

**A tiny native Rust IDE for Java 8 developers learning Rust.**

Rust desktop GUI (egui/eframe), Rust syntax parsing (Tree-sitter), and 12 paired Java/Rust lessons. No JVM, Node.js, Electron, or Chromium runtime.

## Implemented MVP

- Open real `.rs` files and browse real project folders; Rust files remain the source of truth.
- Java-inspired default read-only teaching view (partial, approximate, NOT compilable Java).
- One-click real Rust editor with Save and Ctrl+S.
- Side-by-side Java teaching projection and Rust source.
- Twelve paged Java 8 versus Rust lessons.
- Tree-sitter analysis with line-numbered conceptual hints and syntax-error warning.
- No automatic execution of project code or external network requests.

## Get started on Ubuntu

Install Rust with rustup, plus native GUI dependencies for your distribution, then:

    cargo run
    cargo fmt --check
    cargo check --all-targets
    cargo test
    cargo clippy --all-targets

## Important limitations

Java view is an educational partial projection, not a compiler or semantically equivalent translation. Unsupported Rust syntax remains Rust. It does not fully represent ownership, lifetimes, macros or concurrency semantics. Compare panes currently have independent scrolling. Initial code editing is basic; no LSP, debugger, project builds, synchronized symbol jumps or full IDE features yet. Save writes only the actual opened Rust file.

## Structure

- `src/main.rs`: native UI, editing, file browsing, mode switching
- `src/parser.rs`: Tree-sitter Rust parsing and educational projection
- `src/lessons.rs`: 12 Java 8 versus Rust lessons
- `examples/basics.rs`: demonstration source

MIT licensed. Contributions welcome.
