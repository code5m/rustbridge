# Changelog

## 0.5.0 — Integrated Java-to-Rust learning and lifecycle work

- Private crash-recovery journals, restartable editor/session state, 12 interactive lessons with progress.
- Opt-in real rust-analyzer JSON-RPC LSP hover, completion and definitions with UTF-16 source mappings and no unsolicited edits.
- User-confirmed Cargo check/test/run with cancelable processes, bounded output and clickable rustc diagnostics.
- Cooperative save locking, independent asynchronous read/save workers and slow I/O notices, metadata-preserving macOS/Windows in-place writes with private rollback copies.
- Unix save locking is verified across a real child process; Windows uses an exclusive source handle. The final Linux compare-to-rename race with non-cooperating editors remains an explicit limitation.
- Windows session and crash-journal updates use native MoveFileExW replace-existing/write-through rather than rename-over-existing.
- Incremental UTF-8 line offsets, restart restoration for project/file/mode/lesson/cursor/progress, graceful eframe on_exit session/journal flush and close-during-save lifecycle guards.
- Native CI explicitly exercises real rust-analyzer LSP and Cargo diagnostic subprocess integrations on Linux, macOS and Windows; Linux also tests SIGKILL/relaunch recovery and idle CPU/RSS/fd budgets.
- Incremental UTF-8 line offsets and sliding virtual source editing; fixed the egui scroll visibility idle CPU repaint loop.
- Native Linux GUI/crash/CPU resource smokes, weekly endurance, Windows/macOS CI packaging and draft tag-release workflow.
- Explicit limitations remain on unsigned software, uncooperative writers, uninterruptible OS I/O, cross-platform save atomicity and manual Wayland interaction.

## 0.4.0 — Single-branch finishing pass

- Preserve supported Linux source metadata (mode, owner and readable xattrs/ACL labels) and refuse read-only or unsafe source saves.
- Distinguish a committed rename with uncertain directory durability from a pre-commit failure.
- Move source reading and saving off the GUI thread; cap source size and reject nonregular .rs files without blocking Unix FIFO opens.
- Automatically refresh real directory indexes and reuse one file watcher that detects inode/ctime changes.
- Correct mixed lone-CR/LF/CRLF editing seams; add deterministic randomized Unicode/newline regression tests.
- Introduce bounded AST declaration candidate search for cross-file learning, explicitly not a semantic definition resolver.
- Add Ctrl+1..4 mode switching and Ctrl+PageDown/PageUp large-file navigation for keyboard-first workflow and GUI regression stability.
- Add real GUI keyboard/CRLF save tests, Linux Release resource smoke, Debian .deb packaging and packaged-binary CI validation.
- Keep all v0.4 changes on one review branch before the final single merge to main.

## 0.3.0 — Single-branch integrated correctness and resource hardening

- Fix confirmed Chinese UTF-8 block-comment panic, search-next relative byte offsets, and unsafe custom type-name rewrites.
- Make source saves atomic via temporary-file fsync/rename; protect against common external changes and test rollback on errors.
- Move real directory scanning off UI thread, page long directories, avoid symlink recursion.
- Reuse Tree-sitter incrementally and cancel superseded parses; cache read-only line indices.
- Add bounded 240-line Rust editor pages for large files, plus reversible CRLF/LF display mapping for small and paged editing that preserves untouched source line endings.
- Scope optional diagnostics to its project; cancel process groups, enforce timeouts, and explicitly signal disk-only checks.
- Add Chinese Java structure insights, course explanations and operating-system CJK font fallback.
- Split editor, line ending conversion, filesystem safety, directory indexing, file monitoring and font logic into independent modules.
- Add regression coverage (51 main tests and 13 repeated parser tests in the probe example), console projection probe, real Xvfb screenshots, native keyboard save/CRLF acceptance and performance evaluation.



## 0.2.0 — AST-linked learning views

- Replaced fragile whole-line rewrites with token-level Tree-sitter-based educational transformations.
- Added byte-precise Rust / Java spans and bidirectional token selection, with Unicode-aware line/column positions.
- Added clickable virtualized source panes and selection highlight in both compare panes.
- Added right-side semantic descriptions, Rust token preview and shortcuts to linked compare / Rust editor.
- Linked vertical scrolling (wheel, touchpad, scrollbar drag), opt-out toggle.
- Added source map re-analysis after real Rust edits.
- Indexed AST anchors by source line to avoid per-row full-symbol scanning.
- Added 7 source-mapping unit tests including multi-edit, Unicode/CRLF and incomplete syntax cases.
- Preserved read-only Java projection and the original Rust editing/saving contract.

## 0.1.0 — Initial native IDE MVP

- Built native Rust/egui window, project explorer and basic file editor.
- Added Java-style approximate read-only view, compare, and twelve lessons.
- Initial Tree-sitter syntax guides and Cargo-based CI.
