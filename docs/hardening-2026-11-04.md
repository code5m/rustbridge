# RustBridge v0.3 集中修复与验收记录 — 2026-11-04

## 范围与原则

基线：公开仓库 code5m/rustbridge 的 main 提交 `71315929e6c68a9ce8d210b50b1516887a5482e7`。

交付分支：`fix/rustbridge-integrated-hardening-20261104`。本轮修改集中在一个分支、一次提交；不直接修改 main，也不自动合并。BrowserOS 不在本轮范围内。

真实 `.rs` 源文件是唯一可编辑源。Java 阅读视图仅是教学映射，不能被当作可编译 Java、类型检查结果或反向转换来源。不得将源码中的自定义 `Option`、`Vec` 误认为标准库类型。

## 对审查问题的逐项处理

| 类别 | 原问题 | 处理方式 | 证据 |
|---|---|---|---|
| P0 | 中文/Emoji/嵌套块注释触发 UTF-8 切片 panic | 仅按 ASCII 分隔符字节扫描，不跨字符边界切割 | `editor_core::utf8_regressions` |
| P1 | Find Next 将相对字节位置当作绝对位置 | 将子串查找的相对结果加上起点，并覆盖相邻匹配、中文 | `file_tests::find_next_handles_relative_offsets_and_multibyte_text` |
| P1 | 用户自定义 `Option` 或 `Vec` 被按名称替换 | 仅提示 Java 类比，保留真实类型名称 | `parser::tests::custom_type_names_are_never_rewritten_by_name_only` |
| P1 | 保存直接覆盖、外部修改后易损坏源文件 | 同目录独占临时文件、写入并 fsync、校验文件内容和 inode、原子 rename、同步目录；拒绝符号链接/多硬链接 | `safe_save::tests`，实际 Ctrl+S 和冲突 GUI 验收 |
| P1 | 同步目录扫描占用 GUI 主线程、180 项静默截断 | 新增后台按需目录索引；根目录全部有意义的目录默认可见，超长列表显式分批显示 | `file_tree::tests` 和真实项目打开 |
| P1 | 大文件完整文本与行表每帧重建 | 只读视图复用行偏移索引；源码大于等于 512 KiB 时仅编辑 240 行页；语法高亮和保存只处理展示页的文字 | `editor_core`、Linux GUI 打开 1.55 MB 文件和第二页编辑 |
| P1 | 后台投影整文件重算，旧任务不取消 | Tree-sitter 保留旧 AST 并使用 UTF-8 正确的 InputEdit；parse progress callback + AST 遍历阶段检查取消；只发布最新 revision | `parser::tests` 和 `projection_worker::tests` |
| P1 | rust-analyzer 旧项目结果错位，子进程可能失控 | 诊断任务绑定项目根、项目切换取消、磁盘未保存时阻止诊断、诊断 45s/可用性 5s 超时、Linux 子进程组终止、双管道读取限量 | `analyzer::tests` 和 UI 代码 |
| P1 | 空闲窗口仍每秒重绘检测文件变更 | 移到后台轻量元数据观察线程，仅变化时请求 GUI 重绘 | `file_watch::tests` 与 Release 进程采样 |
| P2 | Java 阅读内容过于 Rust 化、中文显示不完整 | Java AST 结构说明卡片、中英对照课程，加载操作系统中文字体而不打包字体文件 | `parser` 测试及 `docs/images/rustbridge-zh-hardening.png` |
| 追加 | CRLF 大文件分页编辑在回车符之后插入文字 | 编辑器仅显示规范化的 LF，实际写回仅补丁更新原始字节，保留未修改的 CRLF/混合换行 | `editor_eol::tests` 与完整 CRLF GUI 测试，且 Ctrl+S 在明确窗口焦点下成功落盘 |

## 完整性验证

正式交付时应通过：

```bash
cargo fmt --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo build --release
```

- 实际 X11/Xvfb 测试：打开真实 Rust 文件并显示 Java 默认视图、点击双栏锚点、切换原始编辑器、中文字体显示、1.55 MB 源码分页下一页、Ctrl+S 或明确点击保存按钮、外部磁盘修改提示。
- 以操作系统文件 SHA、关键字和 CRLF 计数验证写盘结果，而不只检查窗口是否存活。
- 当前主程序回归数为 51，`examples/projection_probe.rs` 在 `cargo test --all-targets` 下额外重复运行 13 项解析用例；两者不能当成 64 项独立测试。
- **所有 GUI、CI 和 Release 结果以实际运行命令为准；不要将脚本执行视为多平台人工验收。**

## Release 性能结果

测试设备：Linux 7.0.0-38-generic、AMD Ryzen 7 5700U（8 核 16 线程）、14 GiB RAM；共用工作站，Xvfb 1440×900×24。程序启动并预热 8 秒后，读取 `/proc/<pid>/stat` 和 `/proc/<pid>/status` 两端 CPU ticks 与 RSS，采样间隔 5 秒。CPU 为单核口径 `100 × Δticks / (5 × CLK_TCK)`。

| 文件 | 尺寸 | 后台监听前 CPU | 后台监听后 CPU | 新版 RSS | 文件描述符 |
|---|---:|---:|---:|---:|---:|
| basics.rs | 639 B | 3.2% | 0.0% | 125,496 KiB | 10 → 10 |
| glow gl46.rs | 1,546,756 B / 37,803 行 | 3.8% | 0.0% | 181,884 KiB | 10 → 10 |

以上是**一段 5 秒区间内进程 CPU tick 未增长**，不意味着全平台永久 0 CPU。GPU、Wayland、人类持续操作下延迟及长期资源稳定性仍需要独立测试。

使用 `cargo run --release --example projection_probe -- <source.rs>` 的中位耗时（不含文件 IO/GUI）：

| 源码 | 完整投影中位数 | 连续微小修改后增量中位数 |
|---|---:|---:|
| basics.rs | 0.297 ms | 0.108 ms |
| zlib-rs inflate.rs / 102,440 B | 33.223 ms | 15.173 ms |
| glow gl46.rs / 1,546,756 B | 549.924 ms | 146.516 ms |

增量重算和全量重算工作不同；测量用于验收实际交互工作负载，非严格算法等价对照。

## 未夸大的技术边界

1. **原子保存不等于跨进程 CAS**：文件内容和 inode 校验到最终 rename 之间，如果有不配合的并发写入者，仍存在微小 TOCTOU 风险。对需要严格多进程互斥的场景需在未来引入协作式锁/版本确认；当前实现只能阻止普通外部修改、不完整写入及常见冲突。
2. **GUI 文件打开尚可能受慢设备影响**：目录读取已移出 UI，但单个源文件的同步读取仍需在网络盘和超大文件上再评估。后台解析虽然可取消，但正在执行的磁盘 I/O 并没有统一取消能力。
3. **树结构是文件系统真实投影**，并非跨项目的 IDE 索引；文件名搜索有 100 个结果和 12 层深度限制，按需目录树最大 24 层，超过时明确提示。
4. **Java AST 映射是语法层面**：没有跨文件真实类型解析、完整 rust-analyzer LSP、定义跳转或语义等价保障。仅名称相似不应改变源符号。
5. Linux X11/Xvfb 截图和键盘动作不能覆盖所有窗口管理器、Wayland、Windows、Mac 或安装包生命周期；这些是单独的发布准入。
