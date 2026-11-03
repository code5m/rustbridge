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
