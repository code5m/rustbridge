# RustBridge 1.0 learning flow — decisions and acceptance

**Implementation branch:** `main` (the repository's default branch).  
**Scope:** local Java 8 → Rust learning aid, not a general-purpose IDE or transpiler.

## Product decisions

1. **Learn, then practice, then verify.** Reading Java analogies alone is not mastery. Every knowledge point has a conceptual explanation and quiz. Each of six stages provides two editable exercises; compile checking is a distinct metric.
2. **Six stages, 24 concepts.** Stage 1: struct/impl, trait, composition, enum/match, generics, modules. Stage 2: ownership, Move/Copy, exclusive borrowing, lifetimes/Drop. Stage 3: Option, Result, the `?` operator. Stage 4: Vec, HashMap, Mutex/RwLock, Rc/Arc. Stage 5: Cargo, tests, macros. Stage 6: Iterator, Future/async, Pin, unsafe/FFI. Legacy first twelve lesson indices are preserved so older completion state does not change meaning on upgrade.
3. **Trustworthy rustc feedback.** The compiler's JSON `code.code` and source spans are preserved. Help for known error codes (E0382, E0502 etc.) explains Java analogy, actual Rust rule and repair strategies; unknown errors retain original rustc text, not a fabricated explanation.
4. **Exercise editor is separate from source editor.** An exercise is a local scratch buffer, never a rewrite of a real project file. User edits are saved locally with the session; reset requires a click.
5. **Explicit execution consent.** Exercise checks use a new dependency-free temporary Cargo crate, `cargo check --all-targets --offline`, on explicit approval. They compile test targets but do *not* execute tests or binaries. Cargo/rustc and macros are not security sandboxes; warn users to compile only trusted code. Temporary directory is cleaned after completion.
6. **Truthful progress.** Distinguish quiz completion from compile-check success. Both are saved locally. A successful check does not establish test correctness, understanding or runtime safety.
7. **Illustrative Ownership timeline.** Four explicit stages demonstrate creation, move, shared borrow and mutable borrow. This is a fixed teaching example, not static analysis of arbitrary user code.
8. **Navigation history.** F12 Definition stores the originating file and UTF-16 position, enabling Alt+Left return. It must never rewrite source or infer a destination from a name.

## Acceptance requirements

- `cargo fmt --check`, `cargo check --all-targets`, `cargo test --all-targets`, `cargo clippy --all-targets -- -D warnings` all pass.
- Real rust-analyzer LSP integration passes Hover, Completion, Definition tests on a genuine project.
- Each lesson belongs to exactly one stage; all 24 conceptual quizzes have valid options/explanations.
- An isolated practice project is created in a unique restricted temporary directory, and its source exactly matches the user's scratch buffer.
- A real `cargo check --all-targets --offline` must report a missing test-only definition, then accept the corrected version without running the test.
- Session upgrade from twelve legacy lessons preserves original indices; new fields have backward-compatible serde defaults. Quiz attempts, mistakes, per-practice compile success and drafts survive restart.
- Several open source buffers retain independent dirty state, cursor, recovery and save baselines; F12/Alt+Left return to exact locations.
- Actual GUI smoke covers normal edit/save, 35,000-line CRLF paging and document navigation. A headless Xvfb smoke test cannot replace a real interactive desktop check.
- Externally changing a source file blocks unsafe overwrite; cancelling or switching a project never automatically executes code.

## Explicit limitations

- No automatic assessment of whether the learner chose an idiomatic or semantically correct implementation.
- Error explanations are educational and require contextual validation.
- Cargo compile checking may compile code with build-time behavior; use the trust confirmation.
- The Ownership timeline is illustrative; it is not general Rust borrow-checker visualization.
- No guarantee of signed installers or native macOS/Windows visual QA from Linux CI.


## Verification updates and historical evidence

- The original pre-merge learning-path acceptance notes below this heading were superseded by later verification, not by an untested claim: PR #22 (`5447a2c`) passed Windows, macOS and Linux native CI, including real rust-analyzer integration, native Linux 35,000-line GUI editing and SIGKILL-restart-recover-edit-save coverage.
- The complete recovery inventory and asynchronous cold-start audit is tracked separately in `docs/completion-reliability-audit-20261105.md`. It must pass its own CI before being called merged and accepted.
- Actual manual Windows/macOS visual testing, long-duration soak, packaging signing/notarization and power-loss-atomic source replacement remain separate verification or platform boundaries; do not conflate CI tests with those credentials or hardware assurances.
