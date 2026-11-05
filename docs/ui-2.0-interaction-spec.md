# RustBridge UI 2.0 — Interaction Specification

**Status:** Accepted  
**Related decision record:** [UI 2.0 Product & Architecture Decision Record](ui-2.0-decision-record.md)

## 1. First-run and project entry

### No project opened
- Show the bundled example as a clearly labeled read-only learning sample.
- Project navigation explains: "Open a folder to browse the real project; open a .rs file to edit source."
- Java reading remains the default mode.
- Rust Source mode must show a read-only explanation and a disabled editor for the bundled sample. Save must be disabled.
- Do not auto-run Cargo, start rust-analyzer, or index a workspace on startup.

### Open project
1. User selects **Open Project**.
2. If the current source is dirty or saving, show the shared transition dialog.
3. **Save and open:** wait for a durable save; only then switch.
4. **Discard and open:** discard the current recovery journal under explicit user consent; only then switch.
5. **Cancel:** retain the old project, file, buffer, cursor, and dirty state.
6. After confirmation, stop project-scoped background tasks, clear stale diagnostics/results, reset file watcher and directory index, close the old file context, and display the new real filesystem root.
7. A failed directory selection/open leaves the prior project intact.

### Open file
1. User selects a real `.rs` file from Project, Search, Recent, or Open File.
2. Apply the same Save / Discard / Cancel protection.
3. Read asynchronously; keep the previous buffer visible until the new read succeeds.
4. If read fails or is cancelled, retain the previous file and its dirty state.
5. On success, update the file path, source baseline, projection, cursor/session state, and file watcher together.

## 2. Top command bar

Always expose:
- Application name.
- Open Project and Open File.
- Save (enabled only for a real opened file with unsaved edits; saving state shown while pending).
- Work mode: Java Reading, Rust Source, Compare, Learn.
- Navigation visibility.
- Explanation visibility.
- Bottom Tool Window visibility.
- Current file/project and dirty/external-change state.

The bar may wrap on narrow windows. It must not hide the only way to restore a panel.

## 3. Left navigation

### Project
- Display the actual selected root and Cargo.toml detection state.
- Display actual directories and files in filesystem order policy; do not replace paths with tutorial categories.
- Excluded technical directories are hidden only by explicit documented defaults and can be shown on demand.
- Recent files remain secondary to the current real tree.
- File selection always goes through the document transition guard.

### Structure
- Show declarations/concepts extracted from the current Rust AST.
- Each item includes kind and line.
- Clicking an item selects the exact known AST occurrence and navigates the current reading/editor mode.
- Unknown declarations are not synthesized.

### Search
- Search real Rust filenames in the current project.
- Search same-name AST declaration candidates separately; label them as candidates, not resolved definitions.
- Empty and no-result states explain whether a project is required or no match was found.
- Clicking a result uses the document transition guard.

### Learn
- Show all twelve lessons and completion progress.
- Selecting a lesson opens Learn mode and resets quiz feedback only for that lesson.
- Returning from a concept-linked lesson returns to the prior code mode and selected AST anchor.

### Tools
- Show rust-analyzer availability, optional LSP start/stop, and analyzer diagnostics controls.
- State clearly that LSP/diagnostics are opt-in and may index the project.
- Do not mix Cargo run actions into the Project section.

## 4. Central workbench

### Java Reading
- Read-only, syntax-highlighted Java-inspired projection.
- Clickable AST-linked tokens remain visually distinct from ordinary syntax highlighting.
- Display a clear statement that the projection is conceptual and may be incomplete.
- Clicking a token updates the contextual explanation panel.

### Rust Source
- Only a real opened `.rs` file is editable.
- Show real path, dirty state, and external-change warning.
- Hover/type/definition interactions should progressively become editor-native when the LSP implementation supports reliable cursor-position integration; do not simulate automatic behavior with unrelated buttons.
- Large files retain bounded page rendering and absolute source positions.

### Compare
- Java projection on one side; original Rust on the other.
- Default 50/50 split; divider drag clamps each pane to a usable minimum.
- Clicking either side selects the same AST occurrence.
- Linked scrolling is optional.
- On narrow windows, hide side panels before sacrificing either code pane.

### Learn
- Dedicated lesson title and progress.
- Explain the Java mental model, Rust example, key difference, and common mistake.
- Quiz feedback explains why an answer is correct or incorrect.
- Previous/Next navigation preserves completed lesson progress.
- Provide an explicit return-to-code action when entering from a selected concept.

## 5. Contextual explanation panel

- Show Java ↔ Rust conceptual explanation and a conservative-approximation disclaimer.
- Show exact Rust and Java line/column and original/projected token for a selected anchor.
- Offer View Pair, Edit Rust, and Open Lesson where applicable.
- Structure navigation belongs to the left Structure section; do not duplicate the full outline here.
- When no token is selected, explain how to select a highlighted symbol.
- If projection is pending, explain that links are temporarily unavailable.

## 6. Bottom tool window

Tabs:
- **Problems:** Cargo diagnostics with severity, file, line, column, and message. Selecting a problem navigates to the real source location, with dirty-file protection.
- **Build:** Cargo Check and Test. Every action opens the explicit trust confirmation.
- **Run:** Cargo Run. Every action opens the explicit trust confirmation.
- **Output:** bounded recent process output and current task status.

The tool window is resizable and hideable. Starting a Cargo task opens the relevant tab. Cancellation is visible and must terminate the task/process tree according to the existing Cargo runner contract.

## 7. Status and error feedback

- Dirty: unsaved source changes.
- Saving: background save is accepted and still in progress.
- External change: disk content differs; do not overwrite without the existing safe reload/save workflow.
- Durable save: source saved.
- Durability uncertain: content may already be committed; state must not be described as a normal failure.
- Open failure: previous file remains active.
- LSP not started / starting / ready / stopped / failed are distinct.
- Cargo not authorized / awaiting confirmation / running / cancelled / succeeded / failed are distinct.
- No empty panel should be a blank void; show one short next action.

## 8. Keyboard contract

Keep existing shortcuts:
- Ctrl+1: Java Reading.
- Ctrl+2: Rust Source.
- Ctrl+3: Compare.
- Ctrl+4: Learn.
- Ctrl+S: Save current real Rust source.
- Ctrl+PageUp / Ctrl+PageDown: navigate the bounded large-file editor window.

Add shortcuts only when they do not conflict with egui text editing or OS conventions. Every added shortcut must appear in visible help or a tooltip.

## 9. Validation scenarios

- Open a project while clean; verify root, tree, and document context agree.
- Edit a real file; attempt to open another project; test Save, Discard, Cancel.
- Attempt to switch project while save is pending; verify no accepted save is lost.
- Open a broken/unreadable file; verify the old file remains.
- Open the bundled example in Rust Source mode; verify editing and saving are disabled.
- Resize to 1320×820, 1024×768, and 800×550; verify code remains readable and panels can be restored.
- Drag Compare divider to both limits; verify exact AST link selection and linked scrolling.
- Open Learn from a selected symbol; finish a quiz; return to the same code selection.
- Run Check/Test/Run; verify explicit trust confirmation, diagnostics navigation, output, and cancellation.
- Switch projects while LSP/Cargo/diagnostics are active; verify stale results are not shown as current.
