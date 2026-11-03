# RustBridge contributor and implementation contract

## Product intent

RustBridge helps experienced Java 8 programmers understand *actual* Rust code, with a Java-flavored explanatory projection. It is not a new general-purpose IDE, a Rust-to-Java transpiler, or an automatic source rewriter.

## Non-negotiable principles

1. **Real filesystem and Rust source are truth.** Never invent a directory, overwrite real source from the projection, or silently run arbitrary project code.
2. **Java view is derived, read-only, and explicitly approximate.** Do not claim generated Java compiles or preserves all Rust semantics.
3. **Every clickable link must originate from a real Rust AST occurrence.** Keep the same token's exact original and projected byte spans; never infer a source position from a similar display name.
4. **Prefer conservative projections.** Unknown macros, lifetimes, ownership and generic constructs stay in Rust rather than guessed Java syntax. Do not confuse convention-based names with resolved types.
5. **Never mutate files on navigation.** Only edit/save original `.rs` files via explicit user action; warn on unsaved changes before opening another file.
6. **Lightweight by default.** Native Rust UI, no browser/JVM, render visible code rows only, group anchors per line, and keep rust-analyzer optional.
7. **Understandability first.** Clickable concepts, side-by-side examples, exact coordinates and contextual explanation matter more than decorative UI.

## Engineering acceptance for changes to parser/projection

- Rust ↔ Java selected token spans are symmetric, UTF-8 boundaries valid, and line numbers preserved.
- Test field, type, trait, struct, function and multiple tokens on one line.
- Test CRLF, Chinese/Unicode identifiers, macros, malformed/incomplete code and trailing newline.
- Java mapping cannot be used to generate Rust source or save a file.
- Use cargo fmt --check, cargo check --all-targets, cargo test --all-targets, cargo clippy --all-targets -- -D warnings.
- Review desktop UI visually when possible; do not conflate headless process survival with verified interactions.

## Separation of responsibilities

- `parser.rs`: AST extraction / source maps / mapping tests, no GUI.
- `code_view.rs`: click interaction and virtualized token rendering, no filesystem access.
- `main.rs`: window, file I/O, navigation and modes.
- `lessons.rs`: pedagogical concepts only, never use as source rewrite rules.

Roadmap: optional rust-analyzer for resolved identities and go-to-definition; richer source view and diagnostics, not feature-heavy IDE duplication.
