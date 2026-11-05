# RustBridge UI 2.0 — Product & Architecture Decision Record

**Status:** Accepted for implementation on `main`  
**Baseline:** v0.5.0, commit `b1191295e6cd28f4f867f55781e2c4e8a26cbd64`  
**Date:** 2026-11-05

## 1. Product definition

RustBridge is a lightweight native learning IDE for experienced Java 8 developers who are learning to read, understand, and edit real Rust code.

It is not a general-purpose IntelliJ replacement, a Java-to-Rust transpiler, or a Rust-to-Java compiler. The product's distinctive value is the trustworthy bridge between a Java developer's existing mental model and the semantics of actual Rust source.

The actual filesystem and original `.rs` files remain authoritative. The Java view is a derived, read-only, conservative teaching projection. A projection must never be presented as compilable Java or as proof of semantic equivalence.

## 2. Design thesis

**The code workspace is the product; navigation and tools exist to support it.**

UI 2.0 must optimize for the user's next meaningful task, not for exposing every implementation capability at once. The default experience should make it obvious where the project is, which file is open, what can be edited, and how to understand the selected Rust concept.

## 3. Accepted information architecture

The application shell has five regions:

1. **Top command bar** — project/file open, save state, four work modes, and visibility controls.
2. **Left navigation** — one selected section at a time: Project, Structure, Search, Learn, or Tools.
3. **Central workbench** — the primary Java reading, Rust editing, linked Compare, or Learn experience.
4. **Contextual explanation panel** — selected symbol, exact source/projection coordinates, explanation, and related lesson actions. It is optional and collapses on narrow windows.
5. **Bottom tool window** — Problems, Build, Run, and Output. It is hidden until requested or a task needs it; it must not permanently consume the code workspace.

### Left navigation responsibilities

- **Project:** real directory tree, project root, recent files, and excluded-directory visibility. Never invent or flatten filesystem paths.
- **Structure:** AST-backed declarations and concepts from the current real Rust file. Do not claim unresolved symbols are semantically resolved.
- **Search:** real Rust file search and explicitly labeled same-name AST declaration candidates.
- **Learn:** the twelve Java 8 → Rust lessons, completion state, and direct entry to the teaching workspace.
- **Tools:** rust-analyzer status and manual start/stop override, plus explicitly opt-in diagnostics. LSP starts lazily on entry to the real Rust editor in a Cargo project; advanced tools remain discoverable without being mixed into Project.

Cargo Check/Test/Run actions belong to the bottom Build/Run tool windows, not the Project tree.

## 4. Work modes

The four modes remain first-class and keyboard accessible:

- **Java reading:** read-only Java-inspired explanation, exact AST links, conservative mapping.
- **Rust source:** the only editable source surface; real file path and save state must be visible.
- **Compare:** Java projection and original Rust side by side, exact token pairing in both directions, linked scroll, draggable divider.
- **Learn:** a dedicated teaching flow with lesson navigation, Java/Rust examples, conceptual differences, common mistakes, quiz feedback, progress, and a return-to-code path.

Changing modes must not silently change the document, save it, or discard edits.

## 5. Layout and responsive decisions

- Central code area receives priority over side panels.
- Left navigation is resizable and can be hidden.
- Right explanation is resizable, optional, and automatically hidden when the available window width is below the wide-layout threshold (about 1100 px).
- Left navigation can be hidden on narrow windows; the top command bar always exposes a way to restore it.
- Compare uses a draggable split, with a safe minimum width for both panes; default ratio is 50/50.
- Bottom tool window is resizable and toggleable. The status bar remains compact and separate.
- The minimum supported window size must remain usable; no mode may silently compress code panes into unreadable slivers.
- Avoid nested fixed-height scroll areas where a single panel-owned scroll area is sufficient.

## 6. Visual system

Use a restrained native IDE visual language inspired by the clarity of Chrome and the density of professional developer tools:

- Clear hierarchy: application identity, active project/file, work mode, primary content, secondary explanation, status.
- Consistent spacing, panel borders, separators, typography, and selected/hover/focus states.
- Use icons consistently; do not mix emoji, arbitrary Unicode symbols, and unrelated icon styles as decoration.
- Use color semantically: selection/link, success, warning, error, and neutral metadata. Color must not be the only way to convey state.
- Chinese is the default user-facing language; retain Rust/Java technical identifiers and established Cargo/LSP names.
- Keep syntax highlighting readable and conservative. AST-linked tokens remain distinguishable from ordinary syntax-colored text.
- Dark mode remains the default; a light theme may be offered without changing information architecture.
- Avoid decorative cards, excessive borders, gradients, and large headings that reduce code density.

## 7. Interaction decisions

### Project and document lifecycle
- Opening a project is a document lifecycle transition, not a direct assignment to the root path.
- If the current document is dirty or a save is in progress, project switching must use the same explicit Save / Discard / Cancel protection as file switching.
- On confirmed project switch, cancel project-scoped Cargo/LSP/diagnostic tasks, clear stale project results, close the previous document context, reset file watchers and tree indexes, then show the new real root.
- A failed open must preserve the last valid document and project state.
- The bundled example is explicitly read-only. It must never present an enabled editor/save control that cannot persist. A future Save As implementation must use create-new semantics and never overwrite an existing file silently.

### Editing and saving
- Only a real opened `.rs` file is editable.
- Save is explicit and visible; dirty, saving, externally changed, committed-but-durability-uncertain, and failed states must be distinct.
- Never discard accepted background saves or bypass external-change protection.
- Navigation and mode changes never write source files.

### Compare
- A click on either pane selects the exact same AST occurrence in both panes.
- Divider drag changes the pane ratio without changing selection or scroll linkage.
- Linked scrolling remains optional and must not create feedback loops.
- Unknown or approximate mappings remain visibly conservative.

### Tools and feedback
- Opening an application or project must not launch LSP or indexing. rust-analyzer starts lazily only when a user enters the real Rust editor for a Cargo-project file; a manual Stop disables automatic restart until manual Start. Analyzer diagnostics remain opt-in, and no LSP workspace edits or Cargo execution are applied automatically.
- Cargo Check/Test/Run require explicit project trust confirmation every time a new action is started.
- Diagnostics are actionable: clicking a diagnostic navigates to its real file/line when safe; stale results from another project are never shown as current.
- Background work has visible pending, success, warning, failure, and cancellation feedback.

## 8. Accessibility and usability

- All primary actions have visible labels and keyboard equivalents.
- Focus and hover states are visible.
- Text and controls remain legible at the minimum window size.
- Long paths and diagnostics truncate visually but remain available via tooltip or copyable details.
- Empty states explain the next useful action; error states explain what happened and how to recover.
- Avoid ambiguous labels such as "Check" without identifying what is being checked.

## 9. Engineering boundaries

- Keep egui/native Rust; do not introduce Electron, Chromium, JVM, or a webview solely for UI.
- Keep filesystem, parsing, projection, LSP, Cargo, and save work off the rendering thread where already designed.
- Do not weaken the current safety, recovery, CRLF, Unicode, large-file, or resource guarantees to achieve a visual change.
- UI state must not become a second source of truth for the filesystem or document contents.
- A tab strip must not be faked with recent-file buttons. True multi-document tabs require a real Document Manager with per-document buffer, dirty state, save baseline, watcher, recovery, and async I/O ownership. This is implemented in src/document_manager.rs and integrated with DocumentIo and session persistence.

## 10. Explicit non-goals for this UI pass

- Replacing egui with a browser-based frontend.
- Rebuilding RustBridge into a general-purpose IDE with every IntelliJ feature.
- Making Java projection editable or compilable.
- Automatically applying LSP workspace edits.
- Running Cargo or project code on project open.
- Pretending same-name AST candidates are resolved definitions.
- Introducing fake virtual folders or synthetic project structure.

## 11. Acceptance gates

A UI 2.0 change is not complete because it compiles. It must demonstrate:

1. `cargo fmt --check`, `cargo check --all-targets`, `cargo test --all-targets`, and `cargo clippy --all-targets -- -D warnings`.
2. Existing GUI save, CRLF, large-file, recovery, and resource smoke tests remain green.
3. Project switching with a dirty document offers Save / Discard / Cancel and never leaves a file from the old project presented as part of the new project.
4. The bundled example is visibly read-only in Rust Source mode.
5. At wide, medium, and minimum window sizes, the central workspace remains usable; sidebars can be restored.
6. Compare divider is draggable and exact AST selection/linked scrolling still work.
7. Build/Run/Problems/Output are reachable from the bottom tool window and Cargo trust confirmation remains mandatory.
8. A real desktop visual check records any limitation that headless tests cannot verify.

## 12. Delivery policy

This work is implemented directly on the repository's existing default branch `main` (the repository has no `master` branch). Do not create another feature branch for this task. Keep design decisions in this document and keep contributor-facing invariants in `AGENTS.md`. Commit only after the local gates pass; push only a fast-forward update to `origin/main`. Never force-push.
