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
- `main.rs`: window, navigation, document state and lifecycle.
- `safe_save.rs` / `file_watch.rs`: safe saving and off-UI disk change detection.
- `editor_core.rs` / `editor_ui.rs` / `editor_eol.rs`: token coloring, bounded large-source editing and preservation of CRLF/UTF-8 spans.
- `file_tree.rs`: asynchronous real directory reading; no fake directories.
- `lessons.rs`: pedagogical concepts only, never use as source rewrite rules.

Roadmap: optional rust-analyzer for resolved identities and go-to-definition; richer source view and diagnostics, not feature-heavy IDE duplication.


## Hardening expectations (v0.3)

- UTF-8 source slicing must use char boundaries. Scan ASCII punctuation using bytes or validated spans; include Chinese and emoji block comments in regression tests.
- Do not rewrite user-defined Rust identifiers such as Option or Vec based on name alone. A Java analogy needs confirmed semantics, otherwise retain the source name and explain the convention.
- All file save operations act only on original Rust files after explicit action: stage + fsync + compare original bytes and inode + same-directory atomic rename. Keep file permissions. Reject symlink/hardlink replacement; a race with non-cooperating writers remains and must never be claimed solved completely.
- Real directory scans must execute off the UI thread; show pagination rather than silently hide entries. Avoid recursive symlink traversal.
- Use cancellable revision-tagged background parses; check cancellation during Tree-sitter parse and mapping walk. Large editors should only layout a bounded source page and never infer a Java parser from lexical highlights.
- Diagnostics remain opt-in and tied to the current project; stop process groups on cancel/timeout/project change, never show stale project results; warn that analysis reads saved disk files.
- Chinese strings need OS font fallback; never bundle or distribute system font files.
- Measure performance with repeatable workloads. Record idle/active CPU, RSS, file size and hardware; debug Xvfb results are not release desktop acceptance.

- Display CRLF sources as LF only in the editing widget; restore only the changed physical byte span on save, preserving unaffected mixed line endings. Verify insertion, deletion and Unicode cursor offset mapping.


## v0.4 safety and acceptance contracts

- File I/O to open and save actual Rust source happens on the serialized background worker; supersede older opens, do not silently cancel accepted saves. Reject nonregular files and overly large input.
- Never overwrite an owner-readonly file through a parent-directory rename. On Linux, copy and verify available xattrs, file permissions, UID/GID or reject rather than dropping ACL/security metadata. After rename, directory sync failure must be reported as **already committed**.
- File watcher is reusable, compares inode/ctime/size/mtime, and does not create a long-lived thread for every opened file. The directory index automatically revalidates without changing actual path layout.
- Java learning symbol navigation is an AST-only *same-name declaration candidate* service; do not claim actual cross-crate type inference or semantic go-to-definition.
- Release quality requires not only fmt/check/tests/clippy but real X11/Xvfb keyboard saves and CRLF assertions, a bounded resource measurement, and a verified .deb artifact. Do not bundle CJK font files.
- Preserve original files on failures and keep all integrated fixes in one branch/PR until a single gated merge into main.
