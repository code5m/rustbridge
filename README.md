# RustBridge 🦀☕

**一个为 Java 8 开发者设计的极轻量 Rust 学习 IDE。** / A tiny native Rust IDE for Java developers learning Rust.

RustBridge is written in Rust (egui/eframe + Tree-sitter). There is no JVM, Electron, Chromium, or Node.js runtime.

> **Important:** The Java view is an educational *mental model*, not compilable Java, a full transpiler, or a proof of semantic equivalence. The real `.rs` file is the only editable source of truth.

![RustBridge Java / Rust learner view with OS CJK font fallback](docs/images/rustbridge-zh-hardening.png)

## What works — v0.3

- **Java View (default):** open a real `.rs` file and read a conservative AST-based Java-inspired projection.
- **Linked Compare:** left Java projection, right actual Rust; click a highlighted method, field, type or keyword in **either** pane to select its exact corresponding AST token occurrence, highlight both, and jump to the same source location.
- **Semantic guide:** see 1-based Rust / Java line and Unicode-aware column, original/projection token, and Java-to-Rust conceptual explanation.
- **Rust Source:** edit the actual file with syntax colors for keywords, line/block comments and Rust string/character literals, physical line numbers, find-next and native undo/redo shortcuts. Clicking a token shows its explanation; jumping from the guide selects that token in the editor. Save with Ctrl+S.
- **Linked scroll:** optionally follow scrolling across compare panes, including wheel and scrollbar changes.
- **Real filesystem:** open `.rs` files and explore the actual Cargo project tree; do not hide, replace or synthesize source directories.
- **Project navigation:** search real Rust filenames on a cancellable background worker, reopen the eight most recently opened Rust files, show why `.git`, `target` and `node_modules` are excluded by default, and opt in to see them when needed.
- **Safe editing:** saving refuses to overwrite a file changed or removed outside RustBridge; a visible reload action restores the disk copy, while switching files and closing with edits require an explicit save, discard or cancel choice.
- **Responsive projection:** edits are debounced and projected on one on-demand worker. Older results are discarded, so an outdated source map is never applied to newer text.
- **Learn:** 12 paged Java 8 ↔ Rust language concepts, with short code examples.
- **Conservative syntax handling:** Tree-sitter AST recognition, no automatic macro expansion, warnings on malformed syntax; unrecognized Rust syntax stays Rust.
- **Large-file support:** visible-row rendering and per-line indexed AST links, without scanning the entire AST for every visible line.

## v0.3 safety and performance changes

- Prevent editor panic on Chinese or emoji nested block comments; correct search-next byte offsets; preserve custom `Option` / `Vec` type names instead of blindly renaming them.
- Atomic save uses a temporary file in the same directory, fsync and rename; keeps original on staging failures and refuses symlink/hardlink overwrite, external change and inode conflicts. This is **not** a transactional compare-and-swap against uncooperative concurrent editors: a final comparison-to-rename race is still possible.
- Real project directory listing is lazy and asynchronous; entries remain real filesystem paths. Show the complete directory via expandable pages, without silently capping discovery at 180 entries.
- Tree-sitter is reused incrementally after edits and aborted/superseded work is discarded. UI uses cached line indices; Rust files >= 512 KiB use a 240-line paged editor to avoid constructing a full-file text layout every repaint. CRLF is normalized only in the widget and patched back to original source bytes on edits; unchanged mixed line endings stay intact.
- Optional rust-analyzer checks now carry their project identity, stop on switch, cancel their process group and enforce timeouts. Diagnostics operate on saved disk source, not unsaved buffers.
- Chinese beginner explanations, Java structure summaries and a system-font-only CJK fallback improve readability without bundling font files. Unsupported syntax still remains actual Rust.
- Run a local file/folder from the command line: `cargo run -- path/to/real/file.rs` or `cargo run -- /path/to/project`.

## Run locally (Ubuntu/Linux)

Install the Rust toolchain via https://rustup.rs and native X11 / Wayland graphics development libraries required by eframe. On Ubuntu/Debian:

```bash
sudo apt-get install -y build-essential pkg-config libxkbcommon-dev libwayland-dev libx11-dev libx11-xcb-dev libgl1-mesa-dev
git clone https://github.com/code5m/rustbridge.git
cd rustbridge
cargo run
```

Then choose **Open folder** for a real Cargo project or **Open .rs** for a single Rust file. To directly open a known file run `cargo run -- /absolute/path/example.rs`. The Java view opens by default. Click the blue symbols to enter paired comparison. Switch to **Rust Source** when you want to edit and save the original file.

## Validate

```bash
cargo fmt --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

## Performance and limits

Measured observations, machine details and the exact benchmark method are in
[docs/performance-baseline.md](docs/performance-baseline.md). The v0.3 Linux X11/Xvfb run includes visual screenshots and actual click,
keyboard/save and external-change checks, plus reproducible projection probes.
This is targeted native GUI smoke validation, not a multi-platform acceptance claim.

File-name search is bounded to 100 matches and depth 12. The real directory tree
is scanned on a worker and pages entries in groups of 180; all meaningful top-level
directories are initially exposed, and users can expand more entries without a
hidden total cap. Symlinks are displayed, not recursively followed; traversal
depth 24 is reported explicitly. The source editor uses 240-line paging on files
larger than or equal to 512 KiB; the original Rust document remains intact.

## Architecture

| Path | Responsibility |
| --- | --- |
| `src/parser.rs` | Tree-sitter syntax tree, AST occurrence anchors, UTF-8-safe source↔projection span mapping |
| `src/code_view.rs` | Virtualized read-only line renderer and clickable token spans |
| `src/main.rs` | App state and native UI orchestration, view navigation and lifecycle |
| `src/editor_core.rs` / `src/editor_ui.rs` / `src/editor_eol.rs` | UTF-8 safe syntax coloring, bounded pages, CRLF-safe editor buffer mapping |
| `src/file_tree.rs` / `src/file_search_worker.rs` | Lazy real-directory index and cancellable filename search |
| `src/projection_worker.rs` | Reusable incremental Tree-sitter parser and revision cancellation |
| `src/safe_save.rs` | Staged atomic writes and external-editor conflict checks |
| `src/analyzer.rs` | Opt-in process-scoped rust-analyzer diagnostic checks |
| `src/file_watch.rs` | Background disk stamp monitoring, repaint only when data changes |
| `src/ui_fonts.rs` | OS font fallback for Chinese glyphs (no bundled font files) |
| `examples/projection_probe.rs` | Release projection microbenchmark of full vs incremental processing |
| `src/lessons.rs` | 12 introductory Java↔Rust lessons |
| `examples/basics.rs` | Self-contained example |
| `AGENTS.md` | Design principles, contracts, acceptance criteria |

## Precision and scope

**Precise** means the same *syntactic occurrence* has byte-accurate spans in both views. It does **not** yet mean cross-file definition lookup, type inference, trait resolution, lifetime checking, or macro expansion. Analogies such as `Option` ≈ `Optional` and `Vec` ≈ `ArrayList` are **explanatory hints only**: without a real type resolver, Rust identifiers are retained unchanged so a user-defined type cannot be misrepresented. Always check the original Rust and explanatory panel.

Text transforms never reorder source statements or insert/remove newlines, preserving line correspondence. Span maps and Unicode/CRLF handling are unit-tested. Original Rust source is not modified by Java View or Compare.

## Still outside v0.3

- **Optional rust-analyzer diagnostics:** the sidebar can check availability and, on
  demand, run its bounded, cancellable `diagnostics` command for an opened Cargo project.
  Build scripts and proc macros are disabled for this check. It never starts an LSP session;
  completion and go-to-definition remain outside the current scope. The executable is optional;
  its absence never affects core reading or editing.
- No debugging, Rust build/run actions for opened projects, installation bundles, plugin marketplace, or full IDE replacement.
- No complete Java syntax conversion. Java view is intentionally partial and read-only.
- Native window launches and selected GUI interactions are smoke-tested on Linux X11/Xvfb; full human acceptance remains necessary for each desktop environment.
- Atomic save does not provide a perfect lock-free transaction against arbitrary uncooperative external writers; see [the hardening report](docs/hardening-2026-11-04.md) for caveats.

MIT licensed. See [CHANGELOG.md](CHANGELOG.md).
