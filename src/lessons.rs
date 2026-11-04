pub struct Lesson {
    pub java: &'static str,
    pub rust: &'static str,
    pub why: &'static str,
    pub java_code: &'static str,
    pub rust_code: &'static str,
    pub common_mistake: &'static str,
}
pub const LESSONS: &[Lesson] = &[
    Lesson {
        java: "class",
        rust: "struct + impl",
        why: "Separate data from methods; Java inheritance does not carry over.",
        java_code: "class User { String name; }",
        rust_code: "struct User { name: String }\nimpl User { fn name(&self) -> &str { &self.name } }",
        common_mistake: "Expecting struct + impl to provide Java inheritance or nullable fields.",
    },
    Lesson {
        java: "interface",
        rust: "trait",
        why: "Traits define reusable behavior, potentially with default implementations.",
        java_code: "interface Greeter { void greet(); }",
        rust_code: "trait Greeter { fn greet(&self); }",
        common_mistake: "Assuming a trait object or default method has the same dispatch and ownership rules as Java interfaces.",
    },
    Lesson {
        java: "extends",
        rust: "composition + trait",
        why: "Use composition and traits rather than class inheritance.",
        java_code: "class Admin extends User {}",
        rust_code: "struct Admin { user: User }",
        common_mistake: "Looking for extends instead of composing data and implementing shared behavior with traits.",
    },
    Lesson {
        java: "null / Optional",
        rust: "Option<T>",
        why: "Handle Some and None explicitly; missing values are visible in the type.",
        java_code: "Optional<String> s = Optional.empty();",
        rust_code: "let s: Option<String> = None;",
        common_mistake: "Calling unwrap because a Java value would normally be null-checked later.",
    },
    Lesson {
        java: "Exception",
        rust: "Result<T,E>",
        why: "Rust models recoverable failure as explicit success/error values.",
        java_code: "try { load(); } catch(IOException e) {}",
        rust_code: "fn load() -> std::io::Result<String> { std::fs::read_to_string(\"x\") }",
        common_mistake: "Treating Err as a thrown exception without deciding where it is handled or propagated.",
    },
    Lesson {
        java: "ArrayList",
        rust: "Vec<T>",
        why: "An owned, contiguous growable array.",
        java_code: "List<Integer> xs = new ArrayList<>();",
        rust_code: "let mut xs: Vec<i32> = Vec::new(); xs.push(1);",
        common_mistake: "Keeping references into a Vec across a push that may reallocate it.",
    },
    Lesson {
        java: "HashMap",
        rust: "HashMap<K,V>",
        why: "Familiar lookup API, but Rust insertions can move owned keys and values.",
        java_code: "Map<String,Integer> xs = new HashMap<>();",
        rust_code: "let mut xs = std::collections::HashMap::new(); xs.insert(\"a\", 1);",
        common_mistake: "Using a borrowed map entry while mutating the same map.",
    },
    Lesson {
        java: "Stream API",
        rust: "Iterator",
        why: "Iterator chains are lazy, evaluated by a consumer like collect.",
        java_code: "xs.stream().filter(x -> x > 2).collect(toList());",
        rust_code: "let ys: Vec<_> = xs.iter().filter(|x| **x > 2).collect();",
        common_mistake: "Forgetting that iterator adapters are lazy until consumed by collect, for, or another consumer.",
    },
    Lesson {
        java: "CompletableFuture",
        rust: "Future / async",
        why: "Rust futures are lazy and require an executor for progress.",
        java_code: "CompletableFuture.supplyAsync(() -> fetch());",
        rust_code: "async fn fetch() -> String { String::from(\"ok\") }",
        common_mistake: "Assuming calling an async function starts work without an executor awaiting or spawning it.",
    },
    Lesson {
        java: "synchronized",
        rust: "Mutex / RwLock",
        why: "Scoped guards unlock automatically; distinguish std mutex from async mutex.",
        java_code: "synchronized(lock) { count++; }",
        rust_code: "let lock = std::sync::Mutex::new(0); *lock.lock().unwrap() += 1;",
        common_mistake: "Holding a mutex guard across slow work or treating std::sync::Mutex as an async mutex.",
    },
    Lesson {
        java: "JVM + GC",
        rust: "Ownership + Borrowing",
        why: "Rust relies on compile-time ownership checks and deterministic Drop, not a GC.",
        java_code: "User u = new User(); consume(u);",
        rust_code: "let u = String::from(\"hi\"); consume(u); // moved",
        common_mistake: "Using a value after moving it, or adding clones before considering a borrow.",
    },
    Lesson {
        java: "Maven / Gradle",
        rust: "Cargo",
        why: "Cargo handles builds, dependency resolution, tests and packaging.",
        java_code: "mvn test",
        rust_code: "cargo check\ncargo test\ncargo run",
        common_mistake: "Expecting Cargo commands to behave like Maven phases without checking the package and workspace context.",
    },
];

pub struct ChineseLesson {
    pub difference: &'static str,
    pub mistake: &'static str,
}
pub const CHINESE_LESSONS: &[ChineseLesson] = &[
    ChineseLesson {
        difference: "Java 把字段和方法放在 class；Rust 用 struct 保存数据、用 impl 实现方法。不支持 Java 式单继承。",
        mistake: "不要把 struct + impl 误认为 extends，也不能假设字段默认为 null。",
    },
    ChineseLesson {
        difference: "trait 类似 interface，用于约定行为；支持默认方法，但动态派发、所有权和泛型约束与 Java 不同。",
        mistake: "不要把 dyn Trait 当作所有实现类的 Java 多态替代品；要理解静态分发与 trait object。",
    },
    ChineseLesson {
        difference: "Rust 倾向组合数据结构和实现 trait，而不是父子类继承。",
        mistake: "不必寻找 extends；字段组合和特征约束比继承链更自然。",
    },
    ChineseLesson {
        difference: "Option<T> 通过 Some/None 表达有值或缺失，类似 Optional，但与 Java 的 null 模型不同。",
        mistake: "不要靠无条件 unwrap() 取值，优先 match、if let、map 和 ?。",
    },
    ChineseLesson {
        difference: "Result<T,E> 用 Ok/Err 显式传递成功或错误。? 用于传播兼容错误，而非隐式抛异常。",
        mistake: "不要将 Err 当成 Java Exception 直接忽略；要决定在哪里处理或继续向上传播。",
    },
    ChineseLesson {
        difference: "Vec<T> 类似 ArrayList，是拥有元素、连续存储的动态数组；扩容可能移动底层内存。",
        mistake: "不能在 push 引起扩容时保留违反借用规则的引用；无需到处 clone。",
    },
    ChineseLesson {
        difference: "HashMap<K,V> 类似 Map<K,V>，但 insert 可能转移键值所有权，get 往往以借用方式访问。",
        mistake: "不能一边持有同一个 Map 的借用一边进行不兼容的可变修改。",
    },
    ChineseLesson {
        difference: "Iterator 是惰性适配器链，与 Stream API 接近；iter() 借用、iter_mut() 可变借用、into_iter() 消费。",
        mistake: "不要认为 map/filter 会立刻执行，也不要忽略 into_iter() 的所有权影响。",
    },
    ChineseLesson {
        difference: "async fn 返回惰性 Future；需要 executor 通过 await 或 spawn 推进，不等于 CompletableFuture 立即调度。",
        mistake: "调用 async 函数但不 await 或 spawn，通常不会实际执行其中工作。",
    },
    ChineseLesson {
        difference: "Mutex/RwLock 使用作用域 guard 自动释放锁；std::sync 锁与异步锁不能随意混用。",
        mistake: "避免跨越 await 或耗时操作长时间持有同步锁，关注死锁和取消行为。",
    },
    ChineseLesson {
        difference: "Rust 使用所有权、借用检查和 Drop 实现确定性资源管理，而不是依赖 JVM 的 GC。",
        mistake: "值 move 后不能继续用；先考虑借用，再决定是否确实需要 clone。",
    },
    ChineseLesson {
        difference: "Cargo 负责依赖、编译、测试与 workspace 管理，类似 Maven/Gradle，但目标、feature 和执行规则不同。",
        mistake: "不要把 Cargo 命令机械类比为 Maven 生命周期阶段，注意 crate、package、workspace 的区别。",
    },
];
