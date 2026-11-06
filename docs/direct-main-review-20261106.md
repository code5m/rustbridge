# RustBridge 2026-11-06 直接 main 复查与修复验收

范围：基于 `c9cef9e` 复审 AST 折叠/投影、源码保存、持久化退出、Rust 编辑器、目录导航和 GUI 回归。经授权直接在现有 `main` 修改，不创建特性分支、不重写历史。

## 本次修复

- `parser.rs`：AST fold 携带原始 Rust 与 Java 概念投影的字节边界；折叠仅隐藏结构体内部字节，不吞掉结束行的 `} fn sibling()`；保留函数签名。标识符改写禁区从逐叶子节点向上遍历改成自顶向下传递，重复字符串转换使用投影生命周期内的缓存，仍保留语法/UTF-8/Unicode/CRLF 的源映射。
- `code_view.rs`：用原始物理行和片段组成折叠后的虚拟行列表；折叠/展开控制始终可见；支持同一行多结构以及双栏独立折叠，且不修改文件源文本。新增同行第二函数、嵌套折叠、CRLF/中文与大文件测试。
- `main.rs`：单文件 Ctrl+S 只保存接收时的不可变快照，期间继续输入仍为 dirty；只有用户显式 Save All（包括保存并关闭等操作）才持续处理该批次新修订。新增并发输入与 Save All 结果回归。
- `editor_ui.rs`：在 Rust 编辑页提供明确标注的“折叠阅读（只读）”预览，返回编辑仍为完整源文，光标可回到选择处。**不是**直接在折叠行中输入的原位编辑器折叠；因此绝不删除隐藏文本或牺牲撤销/CRLF/字节映射。
- `ui_layout.rs`：代码结构目录每页 60 条，可访问所有 AST 项，不再只显示前 800 条。
- `persistence.rs`：持久化线程退出等待限定 2 秒。对于 OS 不可中断 I/O，打印未确认写入的警告并允许退出，**并不虚构该写入已落盘**；此类极端存储故障依然可能需要人工恢复。新增模拟永久阻塞线程的回归测试。
- `docs/ui-2.0-interaction-spec.md` 与 `AGENTS.md`：统一实际 Save All 行为（单文件冲突不阻塞其他安全的文档保存），明确源文本保护与折叠视图边界。

## 性能测量与结果

测试平台：同一 Ubuntu 开发机，Rust 1.96，`rustc --edition 2024 -C opt-level=s`，`examples/projection_probe.rs`，40,001 行、838,890 字节的 5,000 个函数重复 AST 负载，5 次取中位数。对照版本均由同等编译参数生成。样本有波动，不宜把这组数值等价到全部真实工作区。

| 状态 | 完整投影中位数（毫秒） | 增量投影中位数（毫秒） | 峰值 RSS（KiB） |
| --- | ---: | ---: | ---: |
| 较早的 `e6e43f7` 投影引擎 | 197.174 | 86.066 | 89,900 |
| 新功能初始的 `c9cef9e` | 338.034 | 204.381 | 105,140 |
| 本轮命名缓存/遍历优化版本 | 252.090 | 157.294 | 105,724 |

相对 `c9cef9e`，完整投影约快 25%，增量约快 23%。**没有恢复到 `e6e43f7` 的绝对耗时与内存水平**；新增 AST 结构和链接引入真实成本，不能宣称性能回退完全消失。未来可针对“只在末尾增补注释而 AST 不变”的情形复用只读解析产物，但需同时证明内存与映射正确性，不可为了基准直接跳过解析。

## 已执行的本地验收

`cargo fmt --check`、`cargo check --all-targets`、`cargo test --all-targets`、`cargo clippy --all-targets -- -D warnings`、`cargo test --all-targets -- --ignored --nocapture`（真实 rust-analyzer / Cargo）、`cargo build --release`、Xvfb 的 `tests/gui-smoke.sh`、`tests/compare-fold-gui-smoke.sh`、`tests/recovery-smoke.sh`、`tests/resource-smoke.sh` 全部通过。资源冒烟：闲置单核 CPU 0.00%，RSS 235,492 KiB，FD 10→10。GUI 冒烟包括 CRLF 大文件滚动保存和 SIGKILL 后恢复编辑保存。

## 保留边界

1. Rust 编辑状态中的折叠是只读预览切换，而非完整的可编辑折叠缓冲区/撤销映射引擎。
2. 新 AST 功能比 `e6e43f7` 基线有明确额外 CPU/RSS，尚无充分依据宣称完全解决。
3. 极端 NFS/磁盘不可中断写入时，2 秒退出上限优先保证 GUI 可关闭；任何未确认的持久化写入不能假设成功，恢复日志仍可能丢失。另须长期故障注入/24 小时 Soak。
4. 当前本地 GUI 检测基于 Xvfb，不能代替 Windows/macOS 人工体验、签名或公证。远程 CI 须单独核实。
