# Changelog

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
