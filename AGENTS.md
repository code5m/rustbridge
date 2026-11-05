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


## v0.5 integrated engineering contract

1. Make changes for the 13 approved workstreams in one branch and merge to main once after cross-platform CI. Do not create per-issue main merges or release tags prematurely.
2. No source rewrites from Java projection; only Rust source editing owns actual files. LSP is optional, version-scoped and never executes unsolicited workspace edits.
3. Never execute Cargo/build.rs/tests/project binaries when opening a directory. Every check/test/run requires explicit user trust consent. Stop subprocess groups on cancellation/timeouts.
4. Dirty-source recovery stays local, private and conflict-checked, never overwriting changed on-disk Rust without explicit merge.
5. Accepted saves remain supervised across close requests. Uncooperative writers and OS-level uninterruptible I/O remain documented limits.
6. Large-file layout stays bounded; update source line indexes incrementally; test Unicode/CRLF offsets with randomized workloads.
7. Windows/macOS source writes preserve original identity with private backup, but are not equivalent to Linux power-loss atomic rename. Verify via native CI, and sign/notarize before public binary release.
8. Idle CPU is measured after warm-up and gated against egui repaint loops. Include genuine GUI crash recovery and native interaction in Linux CI, with a scheduled extended soak.


## v0.5 cross-platform lifecycle and release contracts

- Crash journals and session snapshots live in a private per-user state directory, never beside or in place of source files. Recovery restores to a dirty editor buffer only; exports use create-new semantics and never overwrite a user's existing recovery file.
- Windows atomic state snapshots use native replace-existing/write-through semantics. Do not use rename-over-existing where the OS does not guarantee replacement.
- Every save path must distinguish pre-commit failure from committed-but-durability-uncertain state. A successful rename/write is not rolled back by a later directory sync failure.
- On Unix, cooperative RustBridge writers use a per-real-path OS lock; on Windows, an exclusive file handle blocks competing opens. Never claim that advisory locks protect against every non-cooperating external editor or eliminate the final Linux rename race.
- Opening and saving remain on independent background workers. Cancel stale opens, but never silently discard an accepted save. An OS/network filesystem call may remain uninterruptible; communicate slow I/O instead of pretending to cancel it.
- rust-analyzer and Cargo subprocesses are optional. LSP does not apply workspace edits. Cargo check/test/run always require explicit project trust confirmation because build scripts/tests/programs execute user code.
- Run native macOS/Windows CI and packaging, not only Linux cross-compilation. macOS/Windows in-place writes have private rollback copies but are not power-loss atomic; installers are unsigned/unnotarized until signing and notarization are configured.
- Large source editing is a sliding bounded window over the original document, not a full rope editor. Keep absolute UTF-8 byte offsets and CRLF mappings exact across window rebases.


## UI 2.0 product and interaction contract

The binding product and architecture decisions are recorded in `docs/ui-2.0-decision-record.md`; user journeys and acceptance scenarios are in `docs/ui-2.0-interaction-spec.md`. These documents are the source of truth for UI work.

- RustBridge is a focused Java 8 → Rust learning IDE, not a general-purpose IDE clone. The central code workspace is the primary product surface.
- Preserve the actual filesystem and real `.rs` source as the only editable truth. The Java projection remains read-only, conservative, and explicitly educational.
- The left navigation has one active section at a time: Project, Structure, Search, Learn, or Tools. Do not stack Cargo, LSP, lessons, and file tree into one permanently visible panel.
- Keep the central workspace wider than secondary panels. Right-side explanations are contextual and collapsible; bottom Problems/Build/Run/Output is a separate resizable tool window.
- Compare must preserve exact AST token pairing and linked scrolling. A draggable divider may change layout only; it must never change source mapping or selection.
- Opening a project must use the same unsaved/save-in-progress lifecycle guard as opening a file. On project switch, cancel old project tasks and clear stale diagnostics before presenting the new root.
- The bundled example is read-only. Never show an enabled editor/save action for a buffer without a real source path.
- rust-analyzer LSP starts lazily only when the user enters the real Rust editor for a Cargo-project file; application/project startup does not launch indexing. Manual Stop suppresses automatic restart until manual Start. Analyzer diagnostics remain opt-in. Cargo Check/Test/Run always require explicit project trust confirmation; opening a project never executes user code.
- Multi-document tabs are real Document Manager entities, not recent-path buttons. Preserve per-document buffer, dirty baseline, cursor/mode, disk identity, recovery journal and async I/O ownership. Same-project tab switching must never discard another document's unsaved buffer.
- Responsive behavior must collapse secondary panels before making code panes unreadable. Keep the minimum window usable and provide visible controls to restore hidden panels.
- Do not weaken existing safe-save, recovery, CRLF/Unicode, large-file, process-cancellation, or resource guarantees for visual convenience.
- UI changes require unit/regression tests plus a real desktop visual/interaction check when possible. Headless window survival is not proof of visual usability.


## Learning path product and safety contract

- Keep the original twelve lesson indices stable; later lessons are added without invalidating persisted completion data.
- A quiz answer, a successful compiler check, and demonstrated understanding are separate achievements. Never label compile success as semantic correctness or mastery.
- Exercise scratch buffers stay separate from real source files and are persisted locally; a user must approve an isolated no-dependency offline Cargo compile check.
- Parse compiler diagnostic error codes from rustc JSON and attach explanations only for known grounded codes. Unknown errors show the actual compiler text.
- Never auto-run exercise code, users' project tests, or Cargo build scripts. Do not describe rustc or Cargo as a security sandbox.
- Ownership visualizations using the fixed example cannot claim arbitrary program semantic analysis. F12 navigation history must preserve actual paths and source coordinates.
- Learning data must deserialize older sessions with missing fields, and roundtrip across restart. Regression gates in docs/learning-path-v1.md.
