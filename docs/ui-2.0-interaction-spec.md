# RustBridge UI 2.0 — Interaction Specification

**Status:** Accepted  
**Related decision record:** [UI 2.0 Product & Architecture Decision Record](ui-2.0-decision-record.md)

## 1. First-run and project entry

### No project opened
- Show the bundled example as a clearly labeled read-only learning sample.
- Project navigation explains: "Open a folder to browse the real project; open a .rs file to edit source."
- Java reading remains the default mode.
- Rust Source mode must show a read-only explanation and a disabled editor for the bundled sample. Save must be disabled.
- Do not auto-run Cargo or start rust-analyzer on application/project startup. Start rust-analyzer lazily only when the user enters the Rust editor for a real Cargo-project file; manual Stop suppresses auto-restart until the user manually starts it again.

### Open project
1. User selects **Open Project**.
2. If any open document is dirty, saving, or has an unresolved recovery snapshot, show the shared project-transition dialog with the state of every affected document.
3. **Save and open:** save all dirty documents sequentially and wait for durable completion; only then switch.
4. **Discard and open:** discard all dirty buffers only under explicit user consent. Unresolved crash-recovery snapshots must be separately preserved or explicitly deleted.
5. **Cancel:** retain the old project, file, buffer, cursor, and dirty state.
6. After confirmation, stop project-scoped background tasks, clear stale diagnostics/results, reset file watcher and directory index, close the old file context, and display the new real filesystem root.
7. A failed directory selection/open leaves the prior project intact.

### Open file
1. User selects a real `.rs` file from Project, Search, Recent, or Open File.
2. If the file is already open, activate its existing tab. If it is new, open it as another document tab; do not require saving or discarding other tabs.
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

The bar may wrap on narrow windows. It must not hide the only way to restore a panel. Show a real document tab strip with per-tab dirty/external-change markers and a close action; Save All is available when more than one document is dirty.

## 2.1 Document tabs and save lifecycle

- Each tab owns a stable Document ID, source buffer, saved baseline, dirty state, cursor, reading mode, disk identity/external-change state and pending recovery snapshot.
- Reopening an already-open path activates the existing tab; it must not create a duplicate buffer or overwrite unsaved text from disk.
- Switching tabs preserves every document's buffer and dirty state. Opening a new file is asynchronous and does not block the editor.
- Closing a clean tab closes immediately. Closing a dirty tab requires Save / Discard / Cancel for that document only. A save already in flight must finish before the tab closes.
- Save All serializes accepted saves, reports the exact failing document and stops the remaining queue on conflict/failure. No successful save may mark a newer unsaved buffer clean.
- Session persistence records tab order, active path, per-tab cursor/mode and all pending document paths even while asynchronous restore is in progress.
- Restart restores all valid tabs; an unreadable/unsupported entry must not prevent later tabs from restoring. Dirty recovery journals remain attached to their own document and are never silently overwritten by another tab.
- Project switch and application close account for all dirty documents, not only the active editor. Unresolved recovery snapshots must be explicitly preserved or deleted when changing projects.

## 3. Left navigation

### Project
- Display the actual selected root and Cargo.toml detection state.
- Display actual directories and files in filesystem order policy; do not replace paths with tutorial categories.
- Excluded technical directories are hidden only by explicit documented defaults and can be shown on demand.
- Recent files remain secondary to the current real tree.
- Selecting an already-open file activates its existing tab. Selecting a new file in the same project opens a new document without discarding other tabs or their unsaved buffers.

### Structure
- Show declarations/concepts extracted from the current Rust AST.
- Each item includes kind and line.
- Clicking an item selects the exact known AST occurrence and navigates the current reading/editor mode.
- Unknown declarations are not synthesized.

### Search
- Search real Rust filenames in the current project.
- Search same-name AST declaration candidates separately; label them as candidates, not resolved definitions.
- Empty and no-result states explain whether a project is required or no match was found.
- Clicking a result activates its existing document tab or asynchronously opens a new one; other tabs and their dirty buffers remain untouched.

### Learn
- Show all twelve lessons and completion progress.
- Selecting a lesson opens Learn mode and resets quiz feedback only for that lesson.
- Returning from a concept-linked lesson returns to the prior code mode and selected AST anchor.

### Tools
- Show rust-analyzer status and manual start/stop override, plus analyzer diagnostics controls.
- Explain that LSP starts lazily on entry to the real Rust editor in a Cargo project and may index the workspace; analyzer diagnostics remain explicitly opt-in.
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
- Hover displays type information near the pointer; typing triggers debounced Completion; F12 jumps to Definition; Ctrl+Space opens the completion list. These interactions must be driven by editor cursor/pointer events, not separate request buttons. Replies are discarded when the document path, revision or queried position is stale.
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
