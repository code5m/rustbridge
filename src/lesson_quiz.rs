/// One small conceptual assessment per Java→Rust lesson.
/// These check a learner's understanding; they never claim to compile Rust source.
pub struct Quiz {
    pub prompt: &'static str,
    pub choices: [&'static str; 3],
    pub correct: usize,
    pub explanation: &'static str,
}
pub const QUIZZES: &[Quiz] = &[
    Quiz {
        prompt: "Java 的 class User 对应 Rust 哪一种组织方式？",
        choices: [
            "struct 保存数据，impl 定义方法",
            "struct 自动继承父类",
            "impl 自动管理 GC",
        ],
        correct: 0,
        explanation: "struct 和 impl 是分离的；字段不会自动设为 null，也没有 Java 式 extends。",
    },
    Quiz {
        prompt: "Rust 的 trait 与 Java interface 最大的差异是什么？",
        choices: [
            "trait 总是动态分派",
            "trait 可通过泛型静态分派或 dyn 动态分派",
            "trait 必须包含数据字段",
        ],
        correct: 1,
        explanation: "trait 常用于静态分派，只有选择 dyn Trait 等形式才使用动态派发。",
    },
    Quiz {
        prompt: "在 Rust 中复用 User 的数据与行为，通常怎么做？",
        choices: [
            "使用 extends User",
            "依赖隐式继承",
            "组合 User 字段并实现 trait",
        ],
        correct: 2,
        explanation: "Rust 没有 Java 类继承；通过组合结构与 trait 组合能力。",
    },
    Quiz {
        prompt: "Option<String> 是 None 时，哪一种写法更稳妥？",
        choices: [
            "无条件 unwrap()",
            "使用 match 或 if let 处理缺失情况",
            "把 None 当 Java null 直接访问",
        ],
        correct: 1,
        explanation: "Option 将缺失编码在类型中，使用 match/map/if let 可以显式处理 None。",
    },
    Quiz {
        prompt: "Result<T,E> 的 ? 运算符主要做什么？",
        choices: [
            "将 Err 传播给调用方",
            "自动忽略所有错误",
            "抛出与 Java Exception 等价的异常",
        ],
        correct: 0,
        explanation: "? 对 Err 提前返回并进行合适的转换，不代表 JVM 异常传播。",
    },
    Quiz {
        prompt: "向 Vec<T> push 时应该注意什么？",
        choices: [
            "永不移动数据",
            "可能扩容并移动底层缓冲区",
            "Vec 总是像 ArrayList 一样使用引用语义",
        ],
        correct: 1,
        explanation: "Vec 拥有数据，扩容可能分配新内存，借用检查限制不安全的引用存续。",
    },
    Quiz {
        prompt: "HashMap::insert 拿到拥有所有权的 String 时会怎样？",
        choices: [
            "可能移动该 String 到 Map 中",
            "总是拷贝一份 String",
            "把键自动变为静态借用",
        ],
        correct: 0,
        explanation: "Rust 的 insert 通常转移 key/value 所有权；get 以借用获取数据。",
    },
    Quiz {
        prompt: "Rust iterator 的 map/filter 什么时候实际执行？",
        choices: [
            "定义 adapter 的时候",
            "构建 Iterator 的时候立即并行执行",
            "被 for/collect 等消费的时候",
        ],
        correct: 2,
        explanation: "Rust iterator 链是惰性的，和 Java Stream 有相近处，但借用/消费不同。",
    },
    Quiz {
        prompt: "调用一个 async fn 但不 await 或 spawn 会怎样？",
        choices: [
            "通常不会推进其 Future 内部工作",
            "总是立即开启新线程",
            "立即在 JVM 线程池中执行",
        ],
        correct: 0,
        explanation: "Rust Future 是惰性的，需要 executor 和 await/spawn 才会被 poll。",
    },
    Quiz {
        prompt: "MutexGuard 的最佳生命周期通常是？",
        choices: [
            "跨所有 await 保持持锁",
            "把持锁作用域缩小到最小必要区域",
            "永不显式释放",
        ],
        correct: 1,
        explanation: "缩短锁持有时间，不要在同步锁保护下等待网络或耗时工作。",
    },
    Quiz {
        prompt: "String move 到另一个函数后，原变量还能直接用吗？",
        choices: [
            "总是可以",
            "Java GC 会恢复它",
            "不能，除非借用或提前 clone 等适当处理",
        ],
        correct: 2,
        explanation: "所有权 move 后原绑定失效；可借用 &str 或按需 clone。",
    },
    Quiz {
        prompt: "Rust 项目中最适合快速类型检查但不构建可执行产物的是？",
        choices: ["cargo run", "cargo check", "cargo publish"],
        correct: 1,
        explanation: "cargo check 类似 Maven 的快速验证部分流程，cargo test/run 执行代码需要明确授权。",
    },
    Quiz {
        prompt: "Rust 的 match 为何要求穷尽所有 enum 变体？",
        choices: [
            "为了编译器检查所有状态都被处理",
            "因为所有 enum 都是数字",
            "因为 match 自动执行 GC",
        ],
        correct: 0,
        explanation: "穷尽检查降低遗漏状态的风险；变体还可以携带数据。",
    },
    Quiz {
        prompt: "fn f<T: Clone>(x: T) 能依赖哪种能力？",
        choices: [
            "任何 T 自动实现 Copy",
            "T 至少实现 Clone",
            "T 必须是 Java 对象",
        ],
        correct: 1,
        explanation: "trait bound 要求调用方提供满足特征约束的类型。",
    },
    Quiz {
        prompt: "Rust 的 crate 与 Cargo package 关系最准确的是？",
        choices: [
            "一定完全同义",
            "一个 package 可以提供 lib 和 bin 等多个 crate",
            "mod 必然自动读取任意子目录",
        ],
        correct: 1,
        explanation: "Cargo package 是发布/构建单位；crate 是编译单位。",
    },
    Quiz {
        prompt: "哪种赋值使源变量仍可用？",
        choices: [
            "let b = String::from(\"a\"); let c=b; 使用 b",
            "let a=1_i32; let b=a; 继续使用 a",
            "对任意 Vec 使用按值赋值",
        ],
        correct: 1,
        explanation: "i32 实现 Copy，String、Vec 不实现 Copy。",
    },
    Quiz {
        prompt: "为什么同一位置上的活跃 &T 与 &mut T 常会冲突？",
        choices: [
            "Rust 不支持读取可变变量",
            "可变借用要求排他访问",
            "Java JVM 没有 GC",
        ],
        correct: 1,
        explanation: "共享读与排他写的借用规则在编译时保障内存安全。",
    },
    Quiz {
        prompt: "生命周期注解 'a 能做什么？",
        choices: [
            "使局部变量永不被释放",
            "说明多个引用之间的有效性关系",
            "自动创建垃圾回收器",
        ],
        correct: 1,
        explanation: "标注描述关系，不会凭空延长所有权的生命周期。",
    },
    Quiz {
        prompt: "Result<T,E> 后的 ? 遇到 Err 时通常怎样处理？",
        choices: [
            "直接吞掉错误",
            "将错误传播到兼容的函数返回类型",
            "自动重试三次",
        ],
        correct: 1,
        explanation: "? 在失败分支提前返回，并根据 From 等规则转换错误类型。",
    },
    Quiz {
        prompt: "Arc<RefCell<T>> 可以直接保证跨线程共享安全吗？",
        choices: [
            "不一定；RefCell 通常不是 Sync",
            "一定保证",
            "Arc 会禁止全部修改",
        ],
        correct: 0,
        explanation: "Arc 只解决共享所有权；内部类型必须满足并发安全约束。",
    },
    Quiz {
        prompt: "cargo check 与 cargo test 的主要不同是什么？",
        choices: [
            "check 会运行所有单测",
            "test 实际执行测试代码而 check 通常不执行",
            "check 自动发布到 crates.io",
        ],
        correct: 1,
        explanation: "执行测试可能产生副作用，需要显式授权。",
    },
    Quiz {
        prompt: "Rust 的宏展开主要发生在什么时候？",
        choices: ["编译期间", "运行后 JVM 热更新", "操作系统启动时"],
        correct: 0,
        explanation: "macro_rules! 与过程宏处理的是编译期语法。",
    },
    Quiz {
        prompt: "Pin<&mut T> 主要表达什么？",
        choices: [
            "该指针所指值受到移动约束",
            "自动运行 Future",
            "保证一个 T 永不释放",
        ],
        correct: 0,
        explanation: "Pin 约束后续移动，不意味着对象永久存活或 Future 立即执行。",
    },
    Quiz {
        prompt: "unsafe { *ptr } 何时安全？",
        choices: [
            "只要能编译就安全",
            "需要证明指针有效、对齐且访问满足别名等不变量",
            "unsafe 会自动阻止所有空指针",
        ],
        correct: 1,
        explanation: "unsafe 标记人工证明义务，并不提供运行时自动验证。",
    },
];
#[cfg(test)]
mod quiz_tests {
    use super::*;
    #[test]
    fn all_lessons_have_valid_quizzes() {
        assert_eq!(QUIZZES.len(), crate::lessons::LESSONS.len());
        for (lesson, quiz) in QUIZZES.iter().enumerate() {
            assert!(quiz.correct < 3, "invalid quiz for lesson {lesson}");
            assert!(!quiz.prompt.is_empty() && !quiz.explanation.is_empty());
            assert!(quiz.choices.iter().all(|value| !value.is_empty()));
        }
    }
}
