//! Six-stage learning path, deliberately independent of Rust-to-Java projection.
//! All analogies are teaching aids; only cargo/rustc may establish whether code compiles.

use crate::{App, cargo_runner};
use eframe::egui::{self, Color32};
use std::{
    fs, io,
    path::PathBuf,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

pub struct Stage {
    pub name: &'static str,
    pub objective: &'static str,
    pub lesson_indices: &'static [usize],
}

pub const STAGES: [Stage; 6] = [
    Stage {
        name: "01 · 语法与结构迁移",
        objective: "struct、trait、enum、match、泛型和模块；理解不是 Java 语法替换。",
        lesson_indices: &[0, 1, 2, 12, 13, 14],
    },
    Stage {
        name: "02 · 所有权与借用",
        objective: "Move/Copy、共享借用、可变借用、生命周期和 Drop。",
        lesson_indices: &[10, 15, 16, 17],
    },
    Stage {
        name: "03 · 类型与错误传播",
        objective: "Option、Result、? 和显式失败处理。",
        lesson_indices: &[3, 4, 18],
    },
    Stage {
        name: "04 · 集合、内存与并发",
        objective: "Vec、HashMap、Mutex、Rc/Arc 与资源释放。",
        lesson_indices: &[5, 6, 9, 19],
    },
    Stage {
        name: "05 · 真实 Rust 工程",
        objective: "Cargo workspace、测试、宏和可靠构建流程。",
        lesson_indices: &[11, 20, 21],
    },
    Stage {
        name: "06 · 异步与高级边界",
        objective: "Iterator、Future、Pin、unsafe 与 FFI 的适用边界。",
        lesson_indices: &[7, 8, 22, 23],
    },
];

pub fn stage_for_lesson(index: usize) -> usize {
    STAGES
        .iter()
        .position(|stage| stage.lesson_indices.contains(&index))
        .unwrap_or(0)
}

pub struct Exercise {
    pub stage: usize,
    pub title: &'static str,
    pub goal: &'static str,
    pub starter: &'static str,
    pub hint: &'static str,
}

// The examples are intentionally incomplete or incorrect. The learner must
// change the scratch buffer and explicitly approve a local compile check.
pub const EXERCISES: [Exercise; 12] = [
    Exercise {
        stage: 0,
        title: "struct + impl：不要移出借用对象",
        goal: "修正 greeting，让它返回借用的字符串切片，不克隆整个 String。",
        starter: "struct User { name: String }\nimpl User {\n    fn greeting(&self) -> String {\n        self.name\n    }\n}\n",
        hint: "返回类型可以是 &str，表达式可以借用 &self.name。",
    },
    Exercise {
        stage: 0,
        title: "trait：实现约定的方法",
        goal: "让 Dog 正确实现 Speak 的 speak 方法。",
        starter: "trait Speak { fn speak(&self) -> String; }\nstruct Dog;\nimpl Speak for Dog {\n    fn bark(&self) -> String { \"汪\".into() }\n}\n",
        hint: "impl Trait for Type 中的方法签名必须满足 trait 约定。",
    },
    Exercise {
        stage: 1,
        title: "E0382：Move 后仍然使用原变量",
        goal: "保留两个绑定但避免不必要的 clone，让原变量仍可使用。",
        starter: "pub fn sample() -> usize {\n    let name = String::from(\"Rust\");\n    let another = name;\n    println!(\"{}\", name);\n    another.len()\n}\n",
        hint: "让 another 借用 &name，而不是取得 String 的所有权。",
    },
    Exercise {
        stage: 1,
        title: "E0502：可变与不可变借用冲突",
        goal: "保留 push 操作并返回原来的第一个元素，避免悬挂引用。",
        starter: "pub fn sample() -> i32 {\n    let mut values = vec![1, 2];\n    let first = &values[0];\n    values.push(3);\n    *first\n}\n",
        hint: "可以先复制 i32 值，或在 push 之后重新借用。",
    },
    Exercise {
        stage: 2,
        title: "Option：处理缺失值",
        goal: "None 时使用 0，Some 时使用其内部的值，然后加一。",
        starter: "pub fn next(value: Option<i32>) -> i32 {\n    value + 1\n}\n",
        hint: "尝试 value.unwrap_or(0) + 1；这里不是 Java null。",
    },
    Exercise {
        stage: 2,
        title: "Result 与 ?：传播失败",
        goal: "把 parse 的 Result 正确解包并把错误向上返回。",
        starter: "pub fn parse_plus_one(s: &str) -> Result<u32, std::num::ParseIntError> {\n    let value = s.parse::<u32>();\n    Ok(value + 1)\n}\n",
        hint: "在 parse::<u32>() 后使用 ?；该函数已返回 Result。",
    },
    Exercise {
        stage: 3,
        title: "Arc：共享所有权",
        goal: "同时保留两个 Arc 句柄，避免 move 之后使用原值。",
        starter: "use std::sync::Arc;\npub fn len_twice() -> usize {\n    let first = Arc::new(vec![1, 2]);\n    let second = first;\n    first.len() + second.len()\n}\n",
        hint: "Arc::clone(&first) 增加引用计数，不克隆整个 Vec。",
    },
    Exercise {
        stage: 3,
        title: "Mutex：缩短锁的作用域",
        goal: "加锁后递增并返回结果；不要在尾表达式中留下过久的临时守卫。",
        starter: "use std::sync::Mutex;\npub fn counter() -> i32 {\n    let value = Mutex::new(0);\n    *value.lock().unwrap() += 1;\n    *value.lock().unwrap()\n}\n",
        hint: "用独立语句保存结果：let result = *value.lock().unwrap(); result。",
    },
    Exercise {
        stage: 4,
        title: "测试：实现真实行为",
        goal: "让编译器找到 sum，并使内置测试验证 1 + 2 = 3。",
        starter: "#[cfg(test)]\nmod tests {\n    #[test]\n    fn addition() { assert_eq!(super::sum(1, 2), 3); }\n}\n",
        hint: "在模块外添加 pub fn sum(a: i32, b: i32) -> i32 { a + b }。",
    },
    Exercise {
        stage: 4,
        title: "宏：修正格式参数",
        goal: "使 println! 的格式串与参数数量匹配。",
        starter: "pub fn show() {\n    println!(\"value: {}\", 1, 2);\n}\n",
        hint: "每个 {} 占一个参数。也可以删掉多余的参数。",
    },
    Exercise {
        stage: 5,
        title: "Future：await 的位置",
        goal: "把 load 变成异步函数，并等待 fetch 的 Future。",
        starter: "async fn fetch() -> u32 { 42 }\npub fn load() -> u32 { fetch() }\n",
        hint: "async fn load() -> u32 { fetch().await }；调用它仍需 executor。",
    },
    Exercise {
        stage: 5,
        title: "unsafe：优先安全 API",
        goal: "避免直接解引用原始指针；让空切片也可以安全处理。",
        starter: "pub fn first(values: &[i32]) -> Option<i32> {\n    let ptr = values.as_ptr();\n    Some(*ptr)\n}\n",
        hint: "直接返回 values.first().copied()；通过编译不等于完成安全审查。",
    },
];

pub struct Advice {
    pub plain: &'static str,
    pub java: &'static str,
    pub rust: &'static str,
    pub remedy: &'static str,
}

pub fn explain_error(code: &str) -> Option<Advice> {
    let text = match code {
        "E0382" => (
            "使用了已被移动的值",
            "Java 的引用赋值通常只复制引用；GC 不依赖所有权移动。",
            "String、Vec 等非 Copy 值在按值赋值或传参后可能转移所有权。",
            "检查是否应该传入 &T / &mut T；确实需要两份独立数据时再 clone。",
        ),
        "E0502" => (
            "不可变借用与可变借用冲突",
            "Java 允许修改被引用的可变对象，但不会在编译期阻止别名冲突。",
            "有活跃的共享借用时，不能同时创建不兼容的可变借用。",
            "缩短共享借用作用域，复制小的 Copy 值或重新排列操作。",
        ),
        "E0499" => (
            "存在多个同时生效的可变借用",
            "Rust 的 &mut 是排他访问约束，并非 Java 的 synchronized 锁。",
            "在同一作用域内不能同时保持冲突的多个 &mut。",
            "拆开作用域、分割互不相交的字段或顺序操作。",
        ),
        "E0507" => (
            "尝试从借用的数据中移动值",
            "Java getter 往往返回对象引用，不会消费原对象字段。",
            "&self 仅借用了对象，不能擅自移走其非 Copy 字段。",
            "考虑返回引用、显式 clone，或调整方法为 self 消费所有权。",
        ),
        "E0597" => (
            "被借用的值活得不够久",
            "Java 对象存活依赖 GC 可达性；Rust 借用必须在所有权生命周期内有效。",
            "引用不能超出被借用数据或临时锁守卫的有效作用域。",
            "缩短引用作用域、返回拥有所有权的数据，或者提前保存结果。",
        ),
        "E0716" => (
            "临时值在仍被借用时已经释放",
            "Java 临时对象通常由 GC 管理，不存在相同的静态生命周期边界。",
            "临时值在语句结束时被释放，引用无法安全延长其生命。",
            "先绑定拥有所有权的变量，再从变量借用。",
        ),
        "E0106" => (
            "引用缺少生命周期参数",
            "Java 没有 Rust 式借用生命周期标记。",
            "编译器无法判断返回引用与哪个输入借用有关。",
            "提供明确生命周期关系，或改为返回拥有数据的值。",
        ),
        "E0515" => (
            "返回了局部变量或临时值的引用",
            "Java 可以返回堆对象引用；Rust 局部变量在作用域结束时 Drop。",
            "引用不能指向已销毁的局部拥有数据。",
            "返回 String/Vec 等 owned 数据，或让调用方拥有数据后再借用。",
        ),
        "E0308" => (
            "表达式类型不匹配",
            "Java 有自动拆箱等转换；Rust 通常要求显式类型匹配。",
            "实际表达式类型和函数或变量声明不一致。",
            "先检查期待类型与实际类型，必要时显式转换、匹配枚举或 await。",
        ),
        "E0277" => (
            "类型不满足需要的 trait 约束",
            "Java interface 与 Rust trait 不能直接等价，动态/静态分派也不同。",
            "泛型、方法或格式化要求特定 trait 实现。",
            "检查 trait bound、方法接收者和是否真正实现了目标 trait。",
        ),
        "E0599" => (
            "类型上找不到所调用的方法",
            "Java 类方法与扩展机制不同；Rust 方法也可能来自当前作用域中的 trait。",
            "方法不存在、接收者类型不匹配，或者 trait 未引入作用域。",
            "查验具体类型、方法拼写与 trait 的 use/import。",
        ),
        "E0425" => (
            "当前作用域中没有目标名称",
            "Java import 与 Rust 模块路径规则不同。",
            "符号未定义或不在当前模块可见范围。",
            "核查名称、pub 可见性与模块路径。",
        ),
        "E0432" => (
            "无法解析导入的模块或名称",
            "Rust 的 crate/module 路径不等同于 Java package。",
            "use 的模块或符号在当前 crate/依赖图中不存在。",
            "核对 Cargo.toml、crate 根和真实模块路径。",
        ),
        "E0133" => (
            "操作需要显式 unsafe 且证明前置条件",
            "Java 的运行时保护不能直接类比 Rust 的 unsafe 边界。",
            "不安全操作要求由程序员证明指针、边界或并发约束。",
            "优先使用安全 API；仅在能证明全部不变量时才封装最小 unsafe 块。",
        ),
        "E0061" => (
            "调用参数数量不匹配",
            "类型/参数列表和函数真实签名不一致。",
            "Rust 函数签名在编译时严格匹配。",
            "查看定义（F12），改成对应的参数数量与类型。",
        ),
        _ => return None,
    };
    Some(Advice {
        plain: text.0,
        java: text.1,
        rust: text.2,
        remedy: text.3,
    })
}

pub fn create_practice_project(source: &str) -> io::Result<PathBuf> {
    // A real Cargo check is explicitly initiated by the user. This is a
    // temporary standalone no-dependency crate, not an OS sandbox.
    if source.len() > 64 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "练习源码超过 64 KiB",
        ));
    }
    let base = std::env::temp_dir();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos();
    for attempt in 0..16 {
        let dir = base.join(format!(
            "rustbridge-practice-{}-{stamp}-{attempt}",
            std::process::id()
        ));
        match fs::create_dir(&dir) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
                }
                let write = (|| -> io::Result<()> {
                    fs::create_dir(dir.join("src"))?;
                    fs::write(
                        dir.join("Cargo.toml"),
                        "[package]\nname = \"rustbridge_practice\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
                    )?;
                    fs::write(dir.join("src/lib.rs"), source)?;
                    Ok(())
                })();
                if let Err(error) = write {
                    let _ = fs::remove_dir_all(&dir);
                    return Err(error);
                }
                return Ok(dir);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "无法分配独立练习目录",
    ))
}

pub struct PracticeTask {
    pub task: cargo_runner::CargoTask,
    pub root: PathBuf,
    pub exercise: usize,
    pub had_errors: bool,
    pub started: Instant,
}
impl Drop for PracticeTask {
    fn drop(&mut self) {
        // Cancellation may take a moment to release Windows file handles.
        // Cleanup runs outside the GUI thread, including when the user closes
        // the app while an explicitly requested compilation is in progress.
        self.task.cancel();
        let root = self.root.clone();
        std::thread::spawn(move || {
            for _ in 0..30 {
                match fs::remove_dir_all(&root) {
                    Ok(()) => return,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => return,
                    Err(_) => std::thread::sleep(std::time::Duration::from_millis(200)),
                }
            }
        });
    }
}

impl App {
    pub(super) fn poll_practice(&mut self) {
        let Some(running) = self.practice_task.as_ref() else {
            return;
        };
        let mut ended = None;
        let mut diagnostics = Vec::new();
        let mut logs = Vec::new();
        for event in running.task.poll() {
            match event {
                cargo_runner::Event::Diagnostic(diagnostic) => diagnostics.push(diagnostic),
                cargo_runner::Event::Log(line) => logs.push(line),
                cargo_runner::Event::Finished { message, success } => {
                    ended = Some((message, success))
                }
            }
        }
        if let Some(task) = self.practice_task.as_mut() {
            task.had_errors |= diagnostics.iter().any(|d| d.level == "error");
        }
        self.practice_diagnostics.extend(diagnostics);
        for line in logs {
            if self.practice_log.len() > 80 {
                self.practice_log.remove(0);
            }
            self.practice_log.push(line);
        }
        if let Some((message, success)) = ended {
            let task = self.practice_task.take().expect("present");
            let passed = success && !task.had_errors;
            if passed && let Some(done) = self.practice_passed.get_mut(task.exercise) {
                *done = true;
            }
            self.practice_status = if passed {
                format!(
                    "练习编译检查通过：{}。这不是功能测试、语义正确性证明或学习掌握判定。",
                    EXERCISES[task.exercise].title
                )
            } else {
                format!("练习未通过：{message}。请结合诊断和提示修改后重试。")
            };
            let _ = fs::remove_dir_all(&task.root);
        }
    }

    pub(super) fn start_practice(&mut self) {
        self.practice_confirm = false;
        if self.practice_task.is_some() || self.practice_index >= EXERCISES.len() {
            return;
        }
        match create_practice_project(&self.practice_code) {
            Ok(root) => {
                let task = cargo_runner::CargoTask::start(
                    root.clone(),
                    cargo_runner::Action::PracticeCheck,
                );
                self.practice_diagnostics.clear();
                self.practice_log.clear();
                self.practice_status =
                    "Cargo 正在后台检查本地练习（不会运行该练习的 main/test）。".into();
                self.practice_task = Some(PracticeTask {
                    task,
                    root,
                    exercise: self.practice_index,
                    had_errors: false,
                    started: Instant::now(),
                });
            }
            Err(error) => self.practice_status = format!("创建本地练习失败：{error}"),
        }
    }

    pub(super) fn render_practice(&mut self, ui: &mut egui::Ui) {
        let stage = stage_for_lesson(self.lesson);
        let exercises: Vec<_> = EXERCISES
            .iter()
            .enumerate()
            .filter(|(_, exercise)| exercise.stage == stage)
            .collect();
        if self.practice_selected_stage != Some(stage) {
            if self.practice_selected_stage.is_some()
                && let Some(draft) = self.practice_drafts.get_mut(self.practice_index)
            {
                *draft = self.practice_code.clone();
            }
            self.practice_selected_stage = Some(stage);
            self.practice_index = exercises[0].0;
            self.practice_code = self
                .practice_drafts
                .get(self.practice_index)
                .filter(|draft| !draft.is_empty())
                .cloned()
                .unwrap_or_else(|| EXERCISES[self.practice_index].starter.into());
            self.practice_diagnostics.clear();
            self.practice_status.clear();
        }

        ui.separator();
        ui.heading(format!("代码练习 · {}", STAGES[stage].name));
        ui.small("真实 Cargo check --all-targets --offline 检查来自用户明确批准后的本地编译；编译通过并不能证明行为或学习目标正确。");
        ui.horizontal_wrapped(|ui| {
            for (index, exercise) in &exercises {
                let passed = self.practice_passed.get(*index).copied().unwrap_or(false);
                let label = format!("{} {}", if passed { "✓" } else { "○" }, exercise.title);
                if ui
                    .selectable_label(*index == self.practice_index, label)
                    .clicked()
                {
                    if let Some(draft) = self.practice_drafts.get_mut(self.practice_index) {
                        *draft = self.practice_code.clone();
                    }
                    self.practice_index = *index;
                    self.practice_code = self
                        .practice_drafts
                        .get(self.practice_index)
                        .filter(|draft| !draft.is_empty())
                        .cloned()
                        .unwrap_or_else(|| exercise.starter.into());
                    self.practice_diagnostics.clear();
                    self.practice_log.clear();
                    self.practice_status.clear();
                }
            }
        });
        let exercise = &EXERCISES[self.practice_index];
        ui.label(exercise.goal);
        ui.add_enabled(
            self.practice_task.is_none(),
            egui::TextEdit::multiline(&mut self.practice_code)
                .code_editor()
                .font(egui::TextStyle::Monospace)
                .desired_rows(9)
                .desired_width(f32::INFINITY),
        );
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    self.practice_task.is_none(),
                    egui::Button::new("编译检查练习"),
                )
                .clicked()
            {
                self.practice_confirm = true;
            }
            if ui
                .add_enabled(self.practice_task.is_none(), egui::Button::new("重置练习"))
                .clicked()
            {
                self.practice_code = exercise.starter.into();
                if let Some(draft) = self.practice_drafts.get_mut(self.practice_index) {
                    draft.clear();
                }
                self.practice_status = "已恢复练习初始源码。".into();
            }
            ui.collapsing("提示（不直接改代码）", |ui| {
                ui.label(exercise.hint);
            });
            if let Some(task) = &self.practice_task {
                ui.spinner();
                ui.small(format!("检查中 {} 秒", task.started.elapsed().as_secs()));
                if ui.button("取消编译").clicked() {
                    task.task.cancel();
                }
            }
        });
        if !self.practice_status.is_empty() {
            ui.label(&self.practice_status);
        }
        for diagnostic in self.practice_diagnostics.iter().take(8) {
            ui.colored_label(
                if diagnostic.level == "error" {
                    Color32::from_rgb(245, 115, 115)
                } else {
                    Color32::YELLOW
                },
                format!(
                    "{} {} · {}:{} · {}",
                    diagnostic.level,
                    diagnostic.code.as_deref().unwrap_or(""),
                    diagnostic.line,
                    diagnostic.column,
                    diagnostic.message
                ),
            );
            if let Some(advice) = diagnostic.code.as_deref().and_then(explain_error) {
                ui.collapsing(format!("理解错误 · {}", advice.plain), |ui| {
                    ui.strong("Java 心智模型");
                    ui.label(advice.java);
                    ui.strong("Rust 原因");
                    ui.label(advice.rust);
                    ui.strong("推荐修复思路");
                    ui.label(advice.remedy);
                });
            }
        }
        if self.practice_diagnostics.is_empty() && !self.practice_log.is_empty() {
            ui.small(
                self.practice_log
                    .iter()
                    .rev()
                    .take(2)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" | "),
            );
        }
        if self.practice_confirm {
            egui::Modal::new(egui::Id::new("confirm_practice_cargo")).show(ui.ctx(), |ui| {
                ui.heading("确认在本机编译这份练习");
                ui.label("仅编译检查临时无依赖 Cargo 项目和 #[cfg(test)] 代码，不执行其测试或程序，也不访问网络。");
                ui.colored_label(
                    Color32::YELLOW,
                    "Cargo/rustc 并非系统沙箱；只编译你信任的练习代码。",
                );
                ui.horizontal(|ui| {
                    if ui.button("确认执行 Cargo Check").clicked() {
                        self.start_practice();
                    }
                    if ui.button("取消").clicked() {
                        self.practice_confirm = false;
                    }
                });
            });
        }

        let stage_total = exercises.len();
        let stage_done = exercises
            .iter()
            .filter(|(index, _)| self.practice_passed.get(*index).copied().unwrap_or(false))
            .count();
        ui.small(format!(
            "本阶段编译检查通过 {stage_done}/{stage_total} · 六阶段共通过 {}/{}",
            self.practice_passed
                .iter()
                .filter(|&&passed| passed)
                .count(),
            EXERCISES.len()
        ));
        if stage == 1 {
            self.render_ownership_timeline(ui);
        }
    }

    fn render_ownership_timeline(&mut self, ui: &mut egui::Ui) {
        const STEPS: [(&str, &str, &str); 4] = [
            (
                "创建拥有者",
                "a ──拥有──▶ String(\"hello\")",
                "let a = String::from(...) 创建并拥有堆数据，作用域结束时会 Drop。",
            ),
            (
                "所有权移动",
                "a [不能再用]    b ──拥有──▶ String(\"hello\")",
                "let b = a 转移 String 所有权。并非浅拷贝出第二个可用绑定。",
            ),
            (
                "共享借用",
                "a ──拥有──▶ String    r ──&借用──▶ a",
                "let r = &a 不移动所有权；在该共享借用有效期间不能发生冲突的可变借用。",
            ),
            (
                "可变借用",
                "a ──拥有──▶ String    m ──&mut──▶ a",
                "let m = &mut a 排他借用；借用有效期间原变量和其他引用不能进行冲突访问。",
            ),
        ];
        ui.separator();
        ui.strong("所有权变化可视化 · 固定教学示例（不是对任意项目的动态内存分析）");
        ui.horizontal_wrapped(|ui| {
            for (index, (title, _, _)) in STEPS.iter().enumerate() {
                if ui
                    .selectable_label(
                        self.ownership_step == index,
                        format!("{} {}", index + 1, title),
                    )
                    .clicked()
                {
                    self.ownership_step = index;
                }
            }
        });
        let (title, diagram, detail) = STEPS[self.ownership_step];
        ui.group(|ui| {
            ui.strong(title);
            ui.monospace(diagram);
            ui.label(detail);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lessons;
    #[test]
    fn every_lesson_belongs_to_one_stage() {
        assert_eq!(
            STAGES.iter().map(|s| s.lesson_indices.len()).sum::<usize>(),
            lessons::LESSONS.len()
        );
        let mut indices = STAGES
            .iter()
            .flat_map(|s| s.lesson_indices.iter().copied())
            .collect::<Vec<_>>();
        indices.sort_unstable();
        assert_eq!(indices, (0..lessons::LESSONS.len()).collect::<Vec<_>>());
    }
    #[test]
    fn two_practices_per_stage_and_error_codes_are_grounded() {
        for stage in 0..6 {
            assert_eq!(EXERCISES.iter().filter(|p| p.stage == stage).count(), 2);
        }
        assert!(explain_error("E0382").is_some());
        assert!(explain_error("INVALID").is_none());
    }
    #[test]
    fn real_offline_practice_compiles_test_targets_without_running_them() {
        use std::{thread, time::Duration};
        let dir = create_practice_project(
            "#[cfg(test)] mod tests { #[test] fn verify() { super::missing(); } }",
        )
        .unwrap();
        let task = cargo_runner::CargoTask::start(dir.clone(), cargo_runner::Action::PracticeCheck);
        let deadline = Instant::now() + Duration::from_secs(40);
        let mut code_found = false;
        let mut finished = false;
        while Instant::now() < deadline && !finished {
            for event in task.poll() {
                match event {
                    cargo_runner::Event::Diagnostic(d) => {
                        if d.code.as_deref() == Some("E0425") {
                            code_found = true;
                        }
                    }
                    cargo_runner::Event::Finished { message, success } => {
                        assert!(!success, "unexpected success: {message}");
                        finished = true;
                    }
                    cargo_runner::Event::Log(_) => {}
                }
            }
            thread::sleep(Duration::from_millis(30));
        }
        assert!(
            finished && code_found,
            "test-only missing method must fail offline Cargo check"
        );
        drop(task);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn temporary_exercise_project_is_unique_and_contains_exact_source() {
        let a = create_practice_project("pub fn value() -> i32 { 1 }").unwrap();
        let b = create_practice_project("pub fn value() -> i32 { 2 }").unwrap();
        assert_ne!(a, b);
        assert_eq!(
            fs::read_to_string(a.join("src/lib.rs")).unwrap(),
            "pub fn value() -> i32 { 1 }"
        );
        fs::remove_dir_all(a).unwrap();
        fs::remove_dir_all(b).unwrap();
    }
}
