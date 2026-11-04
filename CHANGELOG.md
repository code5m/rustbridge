# Changelog

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
