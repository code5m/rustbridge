# RustBridge 性能、稳定性、可靠性统一整治决策与验收

日期：2026-11-05  
基线：`e8ab474` / `origin/main`  
唯一工作分支：`fix/rustbridge-perf-reliability-unified-20261105`

## 一、目标与约束

本次将性能、崩溃恢复、多文档一致性、LSP 生命周期、跨平台编译检查和 GUI 实测合并为**一个工程变更**，不拆功能分支、不分批合并主分支。代码保留独立于真实 Rust 源文件的只读 Java 教学投影，不引入 JVM 或浏览器，保留真实路径及源代码的唯一事实源。

安全底线：保存必须明确触发，不运行未经授权的 Cargo 程序或 build.rs；不能默默丢弃未保存缓冲区、覆盖外部修改或把测试截图当作真实保存的证据。

## 二、核心架构决策

| 问题 | 决策与实现 | 主要源文件 |
| --- | --- | --- |
| 每帧扫描所有文件判断 dirty | Document 保存缓存的 dirty 标志，只在修改源码或保存基线时计算。当前可见文档在延迟落盘前由 App.live_dirty_count 即时投影其修改状态。 | `document_manager.rs`、`main.rs` |
| 每次打字即时复制整份源码到 Document Manager | GUI 优先更新少量元数据。完整缓冲区仅在约 350ms 崩溃恢复检查点，以及切换、保存、关闭时同步；仍保留独立文档保存基线。 | `main.rs` |
| Hover 无意义的全文克隆 | 直接按借用切片解析显示位置与原始源位置，不在鼠标交互时复制整份源文件。 | `editor_ui.rs` |
| 标签栏每帧访问磁盘 canonicalize | 采用已打开文档的规范路径身份进行快速内存比较，只有边界场景使用 canonicalize。 | `ui_layout.rs`、`document_manager.rs` |
| 每次 AST 点击全表扫描 | 为 Rust 视图和 Java 投影分别建立按物理行划分的跨度索引；宽范围跨度单独处理，不膨胀成海量行元素。保留原有 AST 精确映射语义及渲染锚点。 | `parser.rs` |
| 高频恢复日志无界积压 | 持久化线程按文件路径合并等待中的 Journal 保存/删除请求，仅保留最新 Session；正常退出继续有序排空，没有牺牲显式保存结果。 | `persistence.rs` |
| rust-analyzer 堵塞时发送无界完整源码 | 将未发送编辑按路径与版本收敛为最新快照；工作线程单独取快照。初始化队列保留必要的打开/关闭，避免丢掉文档注册。 | `lsp.rs` |
| LSP 异常退出后就绪状态错乱 | 增加明确的 Stopped 信号、GUI 清理、阻止无限自动重启，用户可在工具面板手动重新启动；只在启动时规范化项目根目录。 | `lsp.rs`、`main.rs` |
| Save All 中一个冲突阻塞其他文件 | 单个保存受理返回结构化结果；保存队列跳过冲突文件并继续后续文件，完整保留未保存内容和错误汇总；失败后不自动关闭窗口。 | `main.rs` |
| Cargo 练习结果依赖平台退出字符串 | 通过子进程 `ExitStatus::success()` 上报结构化 success，Linux/Windows/macOS 不再依赖本地化格式。 | `cargo_runner.rs`、`learning_path.rs` |
| 崩溃恢复后停在只读 Java 页面 | 用户点击恢复后直接进入可编辑 Rust 源视图并定位光标；真实 Xvfb 复测恢复、继续修改、显式保存。 | `main.rs`、`tests/recovery-smoke.sh` |
| 原文件已删除导致恢复快照不可见 | 仍保留合法 Journal，在工具面板独立展示孤立快照并允许选择新目标导出，使用 create-new 禁止覆盖已有文件。 | `persistence.rs`、`main.rs`、`ui_layout.rs` |
| 练习退出可能遗留临时目录 | 编译任务 Drop 时请求取消，并在独立线程尝试删除练习目录，避免 GUI 阻塞。 | `learning_path.rs` |

## 三、验收范围

本机开发机：Ubuntu Linux，直接在授权开发机运行，而非 Codex 沙箱。

验证命令：

```bash
cargo fmt --check
CARGO_INCREMENTAL=0 cargo check --all-targets
CARGO_INCREMENTAL=0 cargo test --all-targets
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
cargo build --release
xvfb-run -a -s '-screen 0 1440x900x24' bash tests/gui-smoke.sh ./target/release/rustbridge
xvfb-run -a -s '-screen 0 1440x900x24' bash tests/recovery-smoke.sh ./target/release/rustbridge
xvfb-run -a -s '-screen 0 1440x900x24' bash tests/resource-smoke.sh ./target/release/rustbridge
```

截至文档编写时，静态检查与单元测试通过；主程序 102 测试通过、0 失败、3 项有意 `#[ignore]` 的实际进程集成测试，后者在 CI 独立运行。大文件 GUI 保存、崩溃恢复 GUI、资源闲置预算均有一次有效通过记录。测试脚本先前的 Xvfb 焦点波动不代表已经证明跨系统无交互问题。

本机 40,000 行资源测试在软件绘制/Xvfb 下采样到 **idle CPU 0.00%（单核折算，5 秒窗口）、RSS 224,796 KiB 不变、10 个文件描述符不变**。这些是观察样本，不是绝对性能承诺。

Windows/macOS/Linux 三系统的 GitHub Actions 必须全部通过，才能考虑一次性合并默认分支 `main`。

## 四、保持诚实的边界与后续审计

1. 编辑器底层仍是有上限的 `String` 与 240 行页面模型，没有宣称已经替换成 Rope/Piece Table。当前优先减少高频全量操作并实测上限；如负载超出 32 MiB 的现有限制，要单独评估结构化文档存储与光标、CRLF、撤销兼容性，不能冒险直接换掉所有编辑器语义。
2. Windows 和 macOS 的原位保存保留原文件身份与私有回滚副本，但不是严格的断电原子替换，不能把这项平台级限制宣称为完全解决。Linux 原子提交与不同进程/外部不合作写入仍有系统级竞争边界。
3. Journal 含原文和编辑文双份快照，启动发现对扫描总读取量有上限；需要进一步优化大规模恢复快照的异步元数据清单与超过容量时的用户提示，避免默默跳过。
4. 24 小时压力测试、真实 Windows/macOS 人工 UI 验收、签名与公证，不因单次 CI 成功就视为已完成。
5. 测试必须证明*真实文件被正确保存*，不可以仅以某个窗口出现、进程存活或虚拟机按键发送作为成功标准。

## 五、合并纪律

仅使用上述唯一分支；合并前对比最新 `origin/main`，不覆盖并行工作。文档和实现必须在同一个 PR 中验收。若 CI 失败，继续在**相同分支**修复，不先把部分更改并入主分支。
