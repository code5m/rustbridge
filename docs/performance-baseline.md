# Performance observations (2026-11-03)

These measurements are exploratory, not a responsiveness or release acceptance claim.
They were taken on Linux 7.0.0-38-generic, AMD Ryzen 7 5700U (8 cores / 16 threads),
14 GiB RAM and 4 GiB swap; the machine was shared with other processes. The RustBridge
checkout contains five Rust source files excluding build outputs. The native release
binary built from commit `7b537ab` was 9,977,360 bytes.

## Projection microbenchmark

An isolated optimized Rust harness compiled both `parser.rs` from main `35ac249` and
from `7b537ab`, using the same locked Tree-sitter versions. It read each source once,
then timed `project` three times in the same process with `Instant`; no GUI, file I/O,
or build time is included. Numbers are milliseconds in execution order:

| Actual Rust source | Bytes / lines | main | `7b537ab` |
| --- | ---: | --- | --- |
| RustBridge `examples/basics.rs` | 639 / 29 | 0.35, 0.24, 0.18 | 0.47, 0.24, 0.20 |
| `zlib-rs` 0.6.8 `src/inflate.rs` | 102,440 / 2,734 | 22.79, 23.57, 22.53 | 33.70, 27.31, 24.71 |
| `glow` 0.16.0 `src/gl46.rs` | 1,546,756 / 37,802 | 371.57, 326.55, 340.10 | 264.32, 232.93, 236.63 |

The newer version deliberately skips opaque macros and produced 60/4,838/55,539
anchors versus 67/5,094/58,766 on main. Thus these are end-to-end projection
comparisons with different work, not a controlled measurement of the mapping
algorithm. Repeated runs and a dedicated harness are needed before setting a
quantitative target. The source files were from the local Cargo registry.

## Preliminary native-process sample

The release binary above was started on the existing Linux display. After startup,
`/proc/<pid>/stat` user+system CPU ticks and `/proc/<pid>/status` VmRSS were sampled
five seconds apart: 0.00% of one CPU core over the interval, 96,168 KiB RSS at both
ends. This is one idle sample with the built-in example, not a main-branch baseline.
The desktop automation interface did not expose the native window, so actual visible
rendering and interaction were not verified.

## Current release re-measurement (commit `530c475`, 2026-11-04)

The same shared machine was used after the background filename search and optional
rust-analyzer diagnostic task were added. `cargo build --release` produced a
10,061,456-byte binary. The application was started with its built-in example and
sampled from `/proc`; after one second of warm-up, CPU ticks (`utime + stime`) and
`VmRSS` were sampled five seconds apart, then the measurement process was terminated.

| Measurement | Observed value | Method / limitation |
| --- | ---: | --- |
| Idle CPU | 0.00% of one core over 5 s | Tick delta divided by `CLK_TCK`; one sample on a shared machine. |
| Idle RSS | 95,304 KiB at both endpoints | `/proc/<pid>/status`; built-in example only. |
| Binary size | 10,061,456 bytes | `stat` after release build. |
| `rust-analyzer diagnostics` | Completed in 20.1 s end-to-end including RustBridge validation commands | Direct command on this small project with build scripts and proc macros disabled; it reported only weak `cfg(test)` inactive-code notices. |

This verifies that native event-loop startup remains idle after initialization and that
the optional diagnostic command can finish on the current project. It is not a GUI
latency benchmark: this environment still does not expose the native window to the
automation driver, so file-open, editing and click latency cannot be measured honestly.

## Still required

Startup latency, repeatable GUI file-open/edit latency, CPU and memory under activity,
real small and medium project navigation, long-running resource-release tests,
comparisons against main for the native application, and a controlled before/after
workload with equivalent mappings remain unmeasured. File scanning and editing still
have synchronous work. The current projection worker debounces requests but does not
perform incremental Tree-sitter updates or cancel a parse already in progress. Do not
interpret the figures above as proof of a performance target.


## RustBridge v0.3 integrated hardening observations — 2026-11-04

These are *additional*, repeatable exploratory measurements of the **uncommitted v0.3 hardening
branch** on the same shared AMD Ryzen 7 5700U / 14 GiB Linux workstation.
The older sections above describe historical commits, not the current code.

### Projection benchmark

Run:

```bash
cargo run --release --example projection_probe -- examples/basics.rs
cargo run --release --example projection_probe -- /path/to/zlib-rs-0.6.8/src/inflate.rs
cargo run --release --example projection_probe -- /path/to/glow-0.16.0/src/gl46.rs
```

The test reads one real source once, measures one cold projection, then
five fresh full projections and five sequential Tree-sitter incremental edits appending
a short comment. The numbers below are the 5-sample median, excluding file I/O and UI
rendering. Full projection and incremental parsing do not perform equivalent work,
so this is a useful comparison of real user workloads, **not** a scientific apples-to-apples
parser optimization benchmark.

| Source | Bytes / physical lines | AST anchors | Cold ms | Full median ms | Incremental median ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| RustBridge basics | 639 / 30 | 61 | 0.481 | 0.297 | 0.108 |
| zlib-rs 0.6.8 inflate.rs | 102,440 / 2,735 | 4,913 | 36.448 | 33.223 | 15.173 |
| glow 0.16.0 gl46.rs | 1,546,756 / 37,803 | 56,954 | 517.362 | 549.924 | 146.516 |

### Native GUI idle resource measurement

Build with `cargo build --release`, run `target/release/rustbridge <real .rs>` on
X11 using an isolated Xvfb 1440×900x24 display, preload the selected file,
wait 8 s, then sample process `utime + stime`, `VmRSS` and number of
`/proc/<pid>/fd` entries, 5 s apart. CPU is **one-core equivalent**
`100 × delta_ticks / (5 × CLK_TCK)`. Debug binaries, Wayland window-manager behavior,
and machine system-wide CPU are not included.

| Workload | Prior 1-second UI repaint polling (sample) | After asynchronous metadata watcher (sample) | After RSS KiB | After FD count |
| --- | ---: | ---: | ---: | ---: |
| basics.rs, small | 3.2% one core | 0.0% one core | 125,496 → 125,496 | 10 → 10 |
| gl46.rs, 1.55 MB | 3.8% one core | 0.0% one core | 181,884 → 181,884 | 10 → 10 |

The process showed no CPU tick changes within the 5 s observation window
following the change to background file metadata observation. This confirms
the **sampled idle** regression is addressed on this machine, not that all
platforms, running projects or longer sessions will sustain 0.00% CPU.
Release binary was approximately 9.7 MiB with the optimized profile.

### Real native GUI coverage and gaps

Xvfb screenshots verified Chinese fonts without bundling font files, a Rust AST-linked
Java reading view, live Rust view, comparison interactions, opening a 37,803-line
real file, selecting page 2 (line 241 onward), and external modification alerts.
Keyboard focus and Ctrl+S were separately verified with an isolated temp source.
Normal desktop integration and reproducible cold-start latency, activity under
continuous typing and scrolling, long-running memory budgets, rapid workspace switching,
and multi-platform tests require separate acceptance.

## RustBridge v0.5 integrated idle and endurance revalidation — 2026-11-04

These measurements use the final integrated v0.5 release binary on the same
Ubuntu workstation, with an actual 40,000–42,000-line Rust source file and an
isolated X11/Xvfb display. CPU is the Linux process user+system tick delta,
expressed as one-core equivalent; it is not whole-machine utilization.

| Workload | Observation | Interpretation |
| --- | ---: | --- |
| 40,000-line idle resource gate | 0.00% one-core CPU over 5 s after 15 s warm-up | No sampled idle repaint loop after scroll-area scrollbar visibility was made stable. |
| 40,000-line idle RSS | 218,540 KiB at both endpoints | Single process, llvmpipe/Xvfb, Tree-sitter warmed. |
| Open file descriptors | 10 → 10 | No growth during the sample. |
| 42,000-line interactive soak | 180 s, 36 keyboard mode-switch samples | Process remained alive; no RSS/FD budget breach. |
| Soak RSS | 207,984 KiB baseline / 214,192 KiB peak | +6,196 KiB peak during repeated view changes. |
| Soak file descriptors | 10 baseline / 10 peak | No descriptor growth. |
| Large-file GUI edit | 35,000-line CRLF file, wheel + keyboard rebasing; saved marker at line 309 | Confirms navigation beyond the first 240-line window and byte-preserving save. |
| Crash recovery | SIGKILL → relaunch → restore → edit → explicit save | The original file stayed unchanged until the user explicitly saved the recovered buffer. |
| GNOME Wayland | xdg_toplevel create/configure and wl_surface commits observed | Native Wayland window lifecycle verified; not a full human IME/accessibility assessment. |

The stable-scrollbar fix addressed a real egui idle repaint regression observed
during v0.5 integration. The values above are repeatable observations on one
machine and renderer, not universal CPU/RSS guarantees. The soak is three minutes,
not a 24-hour soak. The scheduled CI workflow runs a 15-minute weekly Linux soak;
longer 24-hour endurance, real-user multi-platform interaction, and signing/
notarization remain separate release requirements.
