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
