# ADR — Java 概念投影、AST 折叠与结构导航

日期：2026-11-05
仓库主线：`main`
状态：本轮实现与回归约束

## 1. 产品决策

RustBridge 是帮助熟悉 Java 8 的开发者学习 Rust 的轻量 IDE，不是 Rust→Java 转译器。**真实 Rust 路径、文件名、源码、保存内容是唯一事实；Java 面板是可点击、只读、明确标注为不可编译的概念投影**。

不能把 `line_index.rs` 偷换为 `LineIndex.java`，也不能因为文件中出现 `mod` 就说它是内部类。Rust 文件可含自由函数、多个类型、impl、模块和导入。结构导航由真实 Tree-sitter Rust AST 建立，不能依赖 filename 或词法猜测伪造 Java 类型归属。

## 2. 安全且可解释的教学映射

| 真实 Rust | 只读 Java 概念显示 | 规则 |
|---|---|---|
| `snake_case` 方法 / 局部变量 / 字段 / 引用 | `lowerCamelCase` | 仅 ASCII、常规 snake_case；跳过宏、导入路径、模块名称、前导/重复下划线和全大写符号 |
| Rust `type_identifier` 中的常规小写类型名称 | `UpperCamelCase` | 只改展示，不修改类型身份；`String`、`Option` 等类型不凭名称转换成 Java 其他类 |
| `use` | `import` | 仅关键字的**类比**；保留 Rust 路径 `::`，不宣称是 Java import 语法 |
| `mod` | `/*module≈package*/` | 模块/命名空间类比，绝不是 Java 内部类；不改变真实目录树 |
| `struct` | `class`（数据概念） | 方法仍可出现在独立 impl 中，不是 Java 继承 |
| `trait` | `interface`（行为概念） | object safety、默认实现、泛型与 Java 接口不同 |
| `impl T` / `impl Trait for T` | 结构树标成 `T 的方法实现` / `T implements Trait（概念）` | AST 严格识别，绝不伪造 Java 内部类和继承关系 |
| 模块级 `fn` | 自由函数、Java 风格方法名 | 不错误归入不存在的 class |
| `let mut` | `var /*mutable*/`（仅教学记号） | Java 8 没有 var 关键字；`mut` 显式保留为可变绑定注释，绝不宣称是 Java 8 可编译语法 |
| `&`、生命周期、所有权、泛型、宏、编译器语义 | 原样保留，并附带差异说明 | 不能凭字符串改写掩盖 Rust 语义 |

**高亮锚点必须映射原始 Rust AST 的同一个字节出现位置**。行号、UTF-8 字节偏移、反向选中、文件存储不变；仅新投影的视觉字符长度改变，不可把 Java 字符偏移当作 Rust 字符偏移。解析失败或没有明确 AST 上下文时保留 Rust 原文，展示“不确定”而不是生成伪 Java。

## 3. 结构导航与折叠

- 左侧 **结构** 和右侧 **Java 风格结构导航** 均来自 AST 结构清单，覆盖 `use`、`mod`、`struct`、`enum`、`trait`、`impl`、自由函数、成员方法、字段、类型别名与常量。每条显示原始行号、概念标题、层级和点击定位。支持自由函数文件，不能再出现不合理的“0 项”。
- 两栏只读代码视图按 AST 的原始 **物理行范围**提供 `▾/▸`、**全部折叠**、**全部展开**。只可折叠跨行结构；若同一行多个范围竞争，优先外层最大范围和非匿名 block。折叠不更改源码，也不更改物理行号和原来的精确 AST 映射。
- 每个 Document ID 的 Java/Rust 侧分别记忆折叠状态，在 Java 阅读与双栏对照间共享；新源码 hash 变化时失效并重建，避免旧 AST 索引折叠错误内容。
- 非折叠模式保持直接的 `0..line_count` 虚拟行映射；只有用户折叠后才缓存可见行向量。折叠时才更新视图行集合，禁止每帧重新扫描几十万行。
- 点击被折叠范围里的结构或双向链接时，自动展开覆盖该目标的折叠范围；双栏联动滚动必须传递**原始物理行**，不可直接传另一栏的像素偏移；主动折叠引起的布局变化不能被错误判为用户滚动。
- 折叠仅在 Java 概念阅读和 Rust **只读对照**中实现。编辑器使用真实文本和字节安全编辑，不能通过删除 TextEdit 原始字符实现代码折叠。

## 4. 布局决策

对照模式优先保证每栏可读的代码宽度。窄窗口时先自动隐藏右侧解释面板，再收起左导航；在工具栏明确告诉用户为什么隐藏，保持手动开关可发现。单栏模式允许更早显示侧边栏。结构清单独立于右侧栏，不强迫小白长期展示第三列。

## 5. 验收清单

1. `src/line_index.rs` 的自由函数结构导航不再为 0；其 `minimal_diff`、`apply_line_patch`、`old_end` 在 Java 概念视图分别显示为 `minimalDiff`、`applyLinePatch`、`oldEnd`，真实 Rust 源码不变。
2. `use` / `mod` 显示为**类比**，模块不误称内部类；有 struct/trait/impl 时展现清晰的层级与方法归属。
3. ASCII/Unicode/CRLF 多重改写后的 AST 双向定位仍严格对应相同 token。
4. 单个和全部折叠/展开能使行范围隐藏/显示，保留原始物理行号、源字节和邻接函数。跨窗口/跨标签 fold 状态正确归属；折叠中的导航自动展开。
5. 窄窗口布局无严重代码列挤压。GUI 能打开、对照、折叠；旧保存、崩溃恢复、LSP、资源测试不回退。
6. `cargo fmt --check`、`cargo check --all-targets`、`cargo test --all-targets`、`cargo clippy --all-targets -- -D warnings` 以及 Release GUI 冒烟；如推送主线，Linux/Windows/macOS CI 结果如实记录。

## 6. 非目标

不引入 JVM、Java 语法编译器或真实 Rust↔Java 类型解析；不自动重排 Rust 文件、把全部 Rust 函数挪入虚构 class、合并跨文件 impl，不在可编辑 TextEdit 里用删除源文本模拟代码折叠。后续若要实现等价 Java 代码生成，必须单独设计语义分析、诊断和可验证转译契约。
