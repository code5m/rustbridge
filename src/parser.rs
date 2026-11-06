//! Tree-sitter-backed, occurrence-level source maps for the Java reading projection.
//! A projection is a teaching aid, not a Java transpiler or a name resolver.

use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
};
use tree_sitter::{InputEdit, Node, ParseOptions, Parser, Point, Tree};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ByteSpan {
    pub start: usize,
    pub end: usize,
}
impl ByteSpan {
    pub fn contains(self, byte: usize) -> bool {
        self.start <= byte && byte < self.end
    }
    fn len(self) -> usize {
        self.end - self.start
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Location {
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug)]
pub struct Anchor {
    pub rust: ByteSpan,
    pub java: ByteSpan,
    pub rust_at: Location,
    pub java_at: Location,
    pub symbol: String,
    pub kind: &'static str,
    pub topic: &'static str,
    pub explanation: &'static str,
}

#[derive(Clone, Debug)]
pub struct JavaInsight {
    pub rust_byte: usize,
    pub headline: String,
    pub explanation: &'static str,
}

/// A real Rust AST declaration, presented with a Java-friendly *conceptual*
/// title. These are navigation items, never synthetic editable source nodes.
#[derive(Clone, Debug)]
pub struct StructureItem {
    pub rust_byte: usize,
    pub rust_line: usize,
    pub depth: u8,
    pub kind: &'static str,
    pub title: String,
    pub explanation: &'static str,
}

/// Foldable structural span in original/source line coordinates. The teaching
/// projection retains line breaks, so either pane can use the same region.
#[derive(Clone, Debug)]
pub struct FoldRegion {
    pub rust_byte: usize,
    /// Exclusive end of the hidden source span. Closing delimiters are kept visible.
    pub rust_end_byte: usize,
    pub java_byte: usize,
    pub java_end_byte: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub kind: &'static str,
    pub title: String,
}

#[derive(Clone, Debug, Default)]
pub struct Projection {
    pub java: String,
    pub anchors: Vec<Anchor>,
    /// All linked occurrences grouped by their original line; rewrites preserve line breaks.
    pub anchors_by_line: Vec<Vec<usize>>,
    /// Indexed selection in each view. Keep separate from rendering buckets.
    pub rust_lookup_lines: Vec<Vec<usize>>,
    pub java_lookup_lines: Vec<Vec<usize>>,
    pub rust_wide_anchors: Vec<usize>,
    pub java_wide_anchors: Vec<usize>,
    /// Cached offsets used by the virtualized code panes.
    pub rust_line_starts: Vec<usize>,
    pub java_line_starts: Vec<usize>,
    pub java_insights: Vec<JavaInsight>,
    pub structures: Vec<StructureItem>,
    pub folds: Vec<FoldRegion>,
    /// For each original physical line, the outermost fold region that starts here.
    pub fold_starts: Vec<Option<usize>>,
    /// Stable while a document source is unchanged; UI fold state survives tab switches.
    pub source_hash: u64,
    pub warning: bool,
}

impl Projection {
    pub fn rust_anchor(&self, byte: usize) -> Option<usize> {
        self.find(byte, false)
    }
    pub fn java_anchor(&self, byte: usize) -> Option<usize> {
        self.find(byte, true)
    }
    fn find(&self, byte: usize, in_java: bool) -> Option<usize> {
        let (starts, indexed, wide) = if in_java {
            (
                &self.java_line_starts,
                &self.java_lookup_lines,
                &self.java_wide_anchors,
            )
        } else {
            (
                &self.rust_line_starts,
                &self.rust_lookup_lines,
                &self.rust_wide_anchors,
            )
        };
        let line = starts
            .partition_point(|start| *start <= byte)
            .saturating_sub(1);
        indexed
            .get(line)
            .into_iter()
            .flat_map(|items| items.iter())
            .chain(wide.iter())
            .filter_map(|&index| {
                let anchor = self.anchors.get(index)?;
                let span = if in_java { anchor.java } else { anchor.rust };
                span.contains(byte).then_some((index, span.len()))
            })
            .min_by_key(|(_, length)| *length)
            .map(|(index, _)| index)
    }
}

#[derive(Clone)]
struct Edit {
    rust: ByteSpan,
    replacement: String,
}

#[derive(Clone)]
struct Draft {
    rust: ByteSpan,
    kind: &'static str,
    topic: &'static str,
    explanation: &'static str,
    priority: u8,
}

struct Collector<'a> {
    source: &'a str,
    edits: Vec<Edit>,
    drafts: Vec<Draft>,
    seen: HashMap<(usize, usize), usize>,
    cancelled: &'a dyn Fn() -> bool,
    visited: usize,
    insights: Vec<JavaInsight>,
    structures: Vec<StructureItem>,
    folds: Vec<FoldRegion>,
    /// Common local/parameter names repeat thousands of times in large
    /// source files. Reuse their conservative Java display conversion.
    rename_cache: HashMap<(bool, &'a str), Option<String>>,
}

/// UI-only lexical normalization. This does not claim symbol identity,
/// overload resolution or that the projection is valid Java.
pub fn java_camel_case(name: &str) -> Option<String> {
    if !name.contains('_')
        || name.starts_with('_')
        || name.ends_with('_')
        || name.contains("__")
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        || name.bytes().any(|byte| byte.is_ascii_uppercase())
    {
        return None;
    }
    let mut parts = name.split('_');
    let mut result = parts.next()?.to_owned();
    for part in parts {
        let mut letters = part.chars();
        let initial = letters.next()?;
        result.push(initial.to_ascii_uppercase());
        result.extend(letters);
    }
    Some(result)
}

/// Type declarations in Java convention use UpperCamelCase. This is an
/// unambiguous display convention, not a rename of the Rust symbol.
fn java_pascal_case(name: &str) -> Option<String> {
    if name.starts_with('_')
        || name.ends_with('_')
        || name.contains("__")
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        || name.bytes().any(|byte| byte.is_ascii_uppercase())
    {
        return None;
    }
    let camel = java_camel_case(name).unwrap_or_else(|| name.to_owned());
    let mut letters = camel.chars();
    let first = letters.next()?;
    let result = format!("{}{}", first.to_ascii_uppercase(), letters.as_str());
    (result != name).then_some(result)
}

impl<'a> Collector<'a> {
    fn add(
        &mut self,
        node: Node<'_>,
        kind: &'static str,
        topic: &'static str,
        explanation: &'static str,
        priority: u8,
    ) {
        let span = ByteSpan {
            start: node.start_byte(),
            end: node.end_byte(),
        };
        if span.start == span.end {
            return;
        }
        if let Some(&index) = self.seen.get(&(span.start, span.end)) {
            if priority > self.drafts[index].priority {
                self.drafts[index] = Draft {
                    rust: span,
                    kind,
                    topic,
                    explanation,
                    priority,
                };
            }
        } else {
            self.seen.insert((span.start, span.end), self.drafts.len());
            self.drafts.push(Draft {
                rust: span,
                kind,
                topic,
                explanation,
                priority,
            });
        }
    }

    fn keyword(
        &mut self,
        node: Node<'_>,
        keyword: &str,
        replacement: Option<&'static str>,
        topic: &'static str,
        explanation: &'static str,
    ) {
        let mut cursor = node.walk();
        if let Some(token) = node
            .children(&mut cursor)
            .find(|child| child.kind() == keyword)
        {
            self.add(token, "concept", topic, explanation, 3);
            if let Some(replacement) = replacement {
                self.edits.push(Edit {
                    rust: ByteSpan {
                        start: token.start_byte(),
                        end: token.end_byte(),
                    },
                    replacement: replacement.to_owned(),
                });
            }
        }
    }

    fn rewrite_identifier(&mut self, node: Node<'_>, is_type: bool, forbidden: bool) {
        if forbidden {
            return;
        }
        let original: &'a str = node.utf8_text(self.source.as_bytes()).unwrap_or("");
        // Most AST identifiers do not need a Java display rename. Cache
        // repeated names without owning another copy of each source token.
        let renamed = self
            .rename_cache
            .entry((is_type, original))
            .or_insert_with(|| {
                if is_type {
                    java_pascal_case(original)
                } else {
                    java_camel_case(original)
                }
            })
            .clone();
        let Some(replacement) = renamed else {
            return;
        };
        // Rewriter exclusion is inherited as a cheap boolean during AST
        // traversal; don't climb the entire parent chain for each identifier.
        if node.parent().is_some_and(|parent| {
            parent.kind() == "mod_item"
                && parent
                    .child_by_field_name("name")
                    .is_some_and(|name| name.start_byte() == node.start_byte())
        }) {
            return;
        }
        self.edits.push(Edit {
            rust: ByteSpan {
                start: node.start_byte(),
                end: node.end_byte(),
            },
            replacement,
        });
    }

    fn describe_structure(&mut self, node: Node<'_>) {
        let text = |field: &str| {
            node.child_by_field_name(field)
                .and_then(|child| child.utf8_text(self.source.as_bytes()).ok())
        };
        let kind = node.kind();
        let (title, explanation): (String, &'static str) = match kind {
            "use_declaration" => {
                let raw = node.utf8_text(self.source.as_bytes()).unwrap_or("");
                let name = raw
                    .trim()
                    .trim_start_matches("use")
                    .trim()
                    .trim_end_matches(';');
                (
                    format!("import {name}"),
                    "use 类似 Java import，但 Rust 路径、作用域与重导出不同；不是可编译的 Java import。",
                )
            }
            "mod_item" => {
                let name = text("name").unwrap_or("module");
                (
                    format!("模块 {name}（≈ package）"),
                    "Rust mod 定义或声明模块及名称空间；不是 Java 内部类，也不要求一文件一类。",
                )
            }
            "struct_item" => (
                format!(
                    "class {}（数据）",
                    text("name")
                        .map(|name| java_pascal_case(name).unwrap_or_else(|| name.into()))
                        .unwrap_or_else(|| "struct".into())
                ),
                "Rust struct 是数据结构；方法存在独立的 impl 中，不等价于 Java class 继承体系。",
            ),
            "trait_item" => (
                format!(
                    "interface {}（行为）",
                    text("name")
                        .map(|name| java_pascal_case(name).unwrap_or_else(|| name.into()))
                        .unwrap_or_else(|| "trait".into())
                ),
                "Rust trait 类似 Java interface；默认实现、泛型和对象语义并不完全等价。",
            ),
            "enum_item" => (
                format!(
                    "enum {}（可带数据）",
                    text("name")
                        .map(|name| java_pascal_case(name).unwrap_or_else(|| name.into()))
                        .unwrap_or_else(|| "enum".into())
                ),
                "Rust enum 的变体可以携带数据，不等价于所有 Java enum。",
            ),
            "impl_item" => {
                let original_type = text("type").unwrap_or("未知类型");
                let ty = java_pascal_case(original_type).unwrap_or_else(|| original_type.into());
                if let Some(trait_name) = text("trait") {
                    (
                        format!("{ty} implements {trait_name}（概念）"),
                        "impl Trait for Type 是 trait 的实现，不是 Java 的继承或内部类；行为不完全等价。",
                    )
                } else {
                    (
                        format!("{ty} 的方法实现"),
                        "inherent impl 把方法挂到类型上；Rust 可以将数据定义和方法实现分开。",
                    )
                }
            }
            "function_item" | "function_signature_item" => {
                let name = text("name").unwrap_or("function");
                let display = java_camel_case(name).unwrap_or_else(|| name.to_owned());
                let inside_impl = {
                    let mut parent = node.parent();
                    let mut result = false;
                    while let Some(ancestor) = parent {
                        if matches!(ancestor.kind(), "impl_item" | "trait_item") {
                            result = true;
                            break;
                        }
                        parent = ancestor.parent();
                    }
                    result
                };
                (
                    format!(
                        "{} {display}()",
                        if inside_impl {
                            "成员方法"
                        } else {
                            "自由函数"
                        }
                    ),
                    "Rust 函数可在模块作用域独立存在；Java 不支持完全相同的自由函数组织。函数参数、所有权与返回值不能直接翻译。",
                )
            }
            "field_declaration" => {
                let name = text("name").unwrap_or("field");
                (
                    format!(
                        "属性 {}",
                        java_camel_case(name).unwrap_or_else(|| name.into())
                    ),
                    "只把属性标识符按 Java 惯例显示；Rust 字段的所有权/借用不等于 Java 对象属性。",
                )
            }
            "const_item" | "static_item" => (
                format!("常量 {}", text("name").unwrap_or("constant")),
                "Rust const / static 有不同的初始化和存储语义；不能只当作 Java static final。",
            ),
            "type_item" => (
                format!("类型别名 {}", text("name").unwrap_or("type")),
                "Rust type alias 并不创建新类型，与 Java 类型别名和泛型机制不等价。",
            ),
            _ => return,
        };
        let rust_byte = node
            .child_by_field_name("name")
            .map_or(node.start_byte(), |n| n.start_byte());
        let mut depth = 0_u8;
        let mut parent = node.parent();
        while let Some(ancestor) = parent {
            if matches!(
                ancestor.kind(),
                "mod_item" | "impl_item" | "trait_item" | "struct_item" | "enum_item"
            ) {
                depth = depth.saturating_add(1).min(4);
            }
            parent = ancestor.parent();
        }
        self.structures.push(StructureItem {
            rust_byte,
            rust_line: node.start_position().row,
            depth,
            kind: match kind {
                "use_declaration" => "import",
                "mod_item" => "module",
                "struct_item" => "class",
                "trait_item" => "interface",
                "enum_item" => "enum",
                "impl_item" => "impl",
                "function_item" | "function_signature_item" => "function",
                "field_declaration" => "field",
                "const_item" | "static_item" => "constant",
                _ => "type",
            },
            title,
            explanation,
        });
    }

    fn collect_fold(&mut self, node: Node<'_>) {
        let kind = node.kind();
        if !matches!(
            kind,
            "mod_item"
                | "impl_item"
                | "struct_item"
                | "trait_item"
                | "enum_item"
                | "function_item"
                | "function_signature_item"
                | "if_expression"
                | "match_expression"
                | "while_expression"
                | "for_expression"
                | "loop_expression"
                | "use_declaration"
                | "block"
                | "closure_expression"
        ) {
            return;
        }
        let start_line = node.start_position().row;
        let end_line = node.end_position().row;
        if end_line <= start_line + 1 {
            return;
        }
        let title = node
            .child_by_field_name("name")
            .and_then(|child| child.utf8_text(self.source.as_bytes()).ok())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                kind.trim_end_matches("_item")
                    .trim_end_matches("_expression")
                    .into()
            });
        // Preserve the declaration signature and the opening brace; folding
        // only hides the inner body and retains closing braces/siblings.
        let rust_byte = node
            .child_by_field_name("body")
            .filter(|body| body.start_position().row == start_line)
            .map_or(node.start_byte(), |body| {
                body.start_byte().saturating_add(1)
            });
        let mut rust_end_byte = node.end_byte();
        // Keep the matching closing brace visible. Besides being familiar
        // editor behavior, this lets same-line sibling items remain visible.
        if rust_end_byte > rust_byte && self.source.as_bytes().get(rust_end_byte - 1) == Some(&b'}')
        {
            rust_end_byte -= 1;
        }
        if rust_end_byte <= rust_byte {
            return;
        }
        self.folds.push(FoldRegion {
            rust_byte,
            rust_end_byte,
            java_byte: 0,
            java_end_byte: 0,
            start_line,
            end_line,
            kind,
            title,
        });
    }

    fn named(
        &mut self,
        node: Node<'_>,
        kind: &'static str,
        topic: &'static str,
        explanation: &'static str,
    ) {
        if let Some(name) = node.child_by_field_name("name") {
            self.add(name, kind, topic, explanation, 4);
        }
    }

    fn describe_java_structure(&mut self, node: Node<'_>) {
        let name = |field: &str| {
            node.child_by_field_name(field)
                .and_then(|n| n.utf8_text(self.source.as_bytes()).ok())
        };
        let insight = match node.kind() {
            "struct_item" => name("name").map(|n| {
                (
                    node.child_by_field_name("name").unwrap().start_byte(),
                    format!("Java class {n} ≈ Rust struct {n} + impl"),
                    "struct 定义数据字段；impl 才是方法所在位置。它们不是 Java 继承的等价物。",
                )
            }),
            "trait_item" => name("name").map(|n| {
                (
                    node.child_by_field_name("name").unwrap().start_byte(),
                    format!("Java interface {n} ≈ Rust trait {n}"),
                    "trait 表示行为契约；默认实现、trait object 与 Java 接口存在语义区别。",
                )
            }),
            "enum_item" => name("name").map(|n| {
                (
                    node.child_by_field_name("name").unwrap().start_byte(),
                    format!("Rust enum {n} ≈ Java sealed 类型族（近似）"),
                    "Rust enum 变体可以携带数据；不要直接按 Java 枚举常量理解。",
                )
            }),
            "impl_item" => {
                let actual_type = name("type").unwrap_or("未知类型");
                let trait_name = name("trait");
                Some((
                    node.start_byte(),
                    if let Some(t) = trait_name {
                        format!("Java: {actual_type} implements {t}（仅语法类比）")
                    } else {
                        format!("Java: {actual_type} 的方法实现块（impl）")
                    },
                    "Rust 把实现从类型声明中分离；此处不是 Java 类继承。仅根据 AST 结构提供解释。",
                ))
            }
            _ => None,
        };
        if let Some((rust_byte, headline, explanation)) = insight {
            self.insights.push(JavaInsight {
                rust_byte,
                headline,
                explanation,
            });
        }
    }

    fn walk(&mut self, node: Node<'_>, blocked_rewrite: bool) -> bool {
        let blocked_rewrite = blocked_rewrite
            || matches!(
                node.kind(),
                "use_declaration" | "macro_invocation" | "macro_definition" | "attribute_item"
            );
        self.visited += 1;
        if self.visited.is_multiple_of(128) && (self.cancelled)() {
            return false;
        }
        if node.is_error() || node.is_missing() || node.kind() == "macro_invocation" {
            return true;
        }
        if node.has_error() {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if !self.walk(child, blocked_rewrite) {
                    return false;
                }
            }
            return true;
        }
        self.describe_java_structure(node);
        self.describe_structure(node);
        self.collect_fold(node);
        match node.kind() {
            "struct_item" => {
                self.keyword(node, "struct", Some("class"), "class ↔ struct",
                    "A Rust struct stores fields. Behavior lives in separate impl blocks. Java-style inheritance is not available.");
                self.named(node, "type", "Struct declaration",
                    "This is the declaration of a Rust struct; the Java class keyword is only a teaching analogy.");
            }
            "trait_item" => {
                self.keyword(node, "trait", Some("interface"), "interface ↔ trait",
                    "Traits describe shared behavior, can contain default methods, and are not identical to Java interfaces.");
                self.named(node, "trait", "Trait declaration",
                    "This trait defines behavior that a type may implement.");
            }
            "enum_item" => {
                self.keyword(node, "enum", None, "Rust enum ↔ tagged union",
                    "A Rust enum can carry different data per variant. Java enums are only a partial analogy; model each variant explicitly.");
                self.named(node, "type", "Rust enum",
                    "Rust enums can carry data in variants. Java enums are not a complete equivalent.");
            }
            "use_declaration" => self.keyword(node, "use", Some("import"), "use ≈ Java import",
                "Rust use brings a name/path into scope; it is not a byte-for-byte Java import or class loading."),
            "mod_item" => self.keyword(node, "mod", Some("/*module≈package*/"), "mod ≈ module/namespace",
                "Rust mod is a module declaration/namespace, not a Java inner class. Modules may live in files or inline blocks."),
            "impl_item" => self.keyword(node, "impl", None, "impl ↔ method implementation",
                "Impl blocks attach inherent methods or implement a trait; they are not class declarations or inheritance."),
            "function_item" | "function_signature_item" => {
                self.keyword(node, "fn", Some("/*method*/"), "fn ↔ method",
                    "Functions and methods use fn; parameters, return types, ownership and lifetimes differ from Java.");
                self.named(node, "method", "Function or method declaration",
                    "This name identifies the declaration. Inspect &self, &mut self or self for borrow/move semantics.");
            }
            "field_declaration" => self.named(node, "field", "Field declaration",
                "Struct fields hold Rust values directly; their ownership and mutability differ from Java object fields."),
            "let_declaration" => {
                self.keyword(node, "let", Some("var"), "let ↔ local binding",
                    "Java 8 局部变量需要显式类型。此处 var 只是教学标记，不是 Java 8 可编译语法；Rust let 默认不可变，mut 才允许重新绑定。");
                let mut cursor = node.walk();
                if let Some(mutable) = node.children(&mut cursor)
                    .find(|child| child.kind() == "mutable_specifier")
                {
                    self.add(mutable, "concept", "mut ↔ 可变绑定",
                        "Rust mut 表示绑定可变；Java 局部变量默认可重新赋值。所有权和借用规则仍不同。", 3);
                    self.edits.push(Edit {
                        rust: ByteSpan {
                            start: mutable.start_byte(),
                            end: mutable.end_byte(),
                        },
                        replacement: "/*mutable*/".into(),
                    });
                }
            }
            "reference_type" => self.keyword(node, "&", None, "Reference and borrowing",
                "A reference borrows a value; the borrow checker prevents invalid lifetimes and conflicting mutable access."),
            "reference_expression" => self.keyword(node, "&", None, "Reference and borrowing",
                "This expression borrows an existing value. The value remains owned elsewhere, and Rust enforces aliasing rules."),
            "closure_expression" => self.keyword(node, "move", None, "move closure capture",
                "A move closure captures used values by ownership. This differs from Java lambda capture and can make the original binding unavailable."),
            _ => {}
        }

        if node.child_count() == 0 {
            self.leaf(node, blocked_rewrite);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if !self.walk(child, blocked_rewrite) {
                return false;
            }
        }
        true
    }

    fn leaf(&mut self, node: Node<'_>, blocked_rewrite: bool) {
        let text = node.utf8_text(self.source.as_bytes()).unwrap_or("");
        if matches!(node.kind(), "identifier" | "field_identifier") {
            self.rewrite_identifier(node, false, blocked_rewrite);
        } else if node.kind() == "type_identifier" {
            self.rewrite_identifier(node, true, blocked_rewrite);
        }
        match node.kind() {
            "type_identifier" => {
                let (topic, detail, replacement) = match text {
                    "Option" => ("Option<T> ↔ Optional<T>",
                        "若这是标准库 Option<T>，可以类比 Java Optional<T>；未解析到类型身份前保留 Rust 原名，避免误改用户自定义类型。", None),
                    "Vec" => ("Vec<T> ↔ ArrayList<T>",
                        "若这是标准库 Vec<T>，可类比 Java ArrayList<T>；当前尚无 type resolver，原名必须保留。", None),
                    "Result" => ("Result<T,E> ↔ explicit errors",
                        "Result encodes Ok or Err. The ? operator propagates compatible errors; this is not automatic exception throwing.", None),
                    "HashMap" => ("HashMap<K,V> ↔ Map<K,V>",
                        "Map APIs are similar but insertion moves owned keys/values; lookup can borrow.", None),
                    "Mutex" => ("Mutex<T> ↔ synchronized",
                        "Rust mutex locking yields a guard which releases when dropped; poisoning and async usage matter.", None),
                    "RwLock" => ("RwLock<T> ↔ read/write lock",
                        "RwLock allows shared read access or exclusive write access, controlled through guards.", None),
                    "String" => ("String ↔ String",
                        "Rust String owns UTF-8 bytes; Java String is an immutable JVM object. Neither ownership nor encoding is identical.", None),
                    _ => ("Rust type", "The exact type must be resolved before assuming it maps to a Java type.", None),
                };
                self.add(node, "type", topic, detail, 2);
                if let Some(replacement) = replacement {
                    self.edits.push(Edit {
                        rust: ByteSpan { start: node.start_byte(), end: node.end_byte() },
                        replacement,
                    });
                }
            }
            "field_identifier" => {
                let is_decl = node.parent().is_some_and(|p| p.kind() == "field_declaration");
                let (topic, detail) = match text {
                    "iter" | "iter_mut" | "into_iter" => ("Iterator traversal", "Iterator methods express borrowed, mutable-borrowed, or owned traversal. This is not a Java Stream conversion and may move elements."),
                    "map" | "filter" | "collect" => ("Iterator pipeline", "Rust iterator adapters are lazy until consumed. Borrowing and ownership in the closure still apply."),
                    _ if is_decl => ("Field declaration", "This is the declared field name. The paired Rust span is its exact AST occurrence."),
                    _ => ("Field or method access", "This is a field or method occurrence, not necessarily the declaration. Cross-file name resolution is not implemented."),
                };
                self.add(node, "field",
                    topic, detail, 1);
            }
            "identifier" => self.add(node, "symbol", "Rust symbol occurrence",
                "Clicking links this exact token occurrence between the two views. Definition lookup requires a type/name resolver.", 1),
            "primitive_type" => self.add(node, "type", "Primitive type",
                "Rust integer and floating-point types have explicit sizes; Java primitive names are not always equivalent.", 1),
            "lifetime" => self.add(node, "lifetime", "Lifetime parameter",
                "Lifetimes constrain references and cannot be directly represented by a Java type annotation.", 1),
            "self" => self.add(node, "receiver", "self ↔ this",
                "Rust self can be moved, borrowed immutably (&self), or mutably (&mut self); Java this is not equivalent.", 1),
            _ => {}
        }
    }
}

fn apply_edits(source: &str, edits: &[Edit]) -> String {
    let mut result = String::with_capacity(source.len());
    let mut cursor = 0;
    for edit in edits {
        result.push_str(&source[cursor..edit.rust.start]);
        result.push_str(&edit.replacement);
        cursor = edit.rust.end;
    }
    result.push_str(&source[cursor..]);
    result
}

fn shifted(value: usize, offset: isize) -> usize {
    value
        .checked_add_signed(offset)
        .expect("valid projection offset")
}

fn map_offset(byte: usize, edits: &[Edit], offsets: &[isize]) -> usize {
    let index = edits.partition_point(|edit| edit.rust.end <= byte);
    shifted(byte, offsets[index])
}

fn map_span(span: ByteSpan, edits: &[Edit], offsets: &[isize]) -> Option<ByteSpan> {
    let index = edits.partition_point(|edit| edit.rust.end <= span.start);
    let delta = offsets[index];
    if let Some(edit) = edits.get(index).filter(|edit| edit.rust.start < span.end) {
        if edit.rust != span {
            // Never present a partial or overlapping rewrite as a precise source map.
            return None;
        }
        let start = shifted(span.start, delta);
        return Some(ByteSpan {
            start,
            end: start + edit.replacement.len(),
        });
    }
    Some(ByteSpan {
        start: shifted(span.start, delta),
        end: shifted(span.end, delta),
    })
}

pub fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (byte, _) in source.match_indices('\n') {
        starts.push(byte + 1);
    }
    starts
}

fn locate(source: &str, starts: &[usize], byte: usize) -> Location {
    let line = starts
        .partition_point(|&begin| begin <= byte)
        .saturating_sub(1);
    Location {
        line,
        column: source[starts[line]..byte].chars().count(),
    }
}

pub fn char_to_byte(source: &str, char_index: usize) -> usize {
    source
        .char_indices()
        .nth(char_index)
        .map_or(source.len(), |(byte, _)| byte)
}

/// Incremental Tree-sitter parser scoped to one file/document owner.
pub struct ProjectionEngine {
    parser: Parser,
    previous: Option<(String, Tree)>,
}

impl ProjectionEngine {
    pub fn new() -> Result<Self, String> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .map_err(|error| format!("Rust grammar unavailable: {error}"))?;
        Ok(Self {
            parser,
            previous: None,
        })
    }

    /// Returns None when superseded/cancelled. A failed parse never poisons the old tree.
    pub fn project(&mut self, source: &str, cancelled: &dyn Fn() -> bool) -> Option<Projection> {
        if cancelled() {
            return None;
        }
        let previous_tree = self.previous.as_ref().map(|(old, tree)| {
            let mut tree = tree.clone();
            if old != source {
                tree.edit(&minimal_edit(old, source));
            }
            tree
        });
        let mut progress = |_state: &tree_sitter::ParseState| cancelled();
        let options = ParseOptions::new().progress_callback(&mut progress);
        let mut input = |byte: usize, _point: Point| &source.as_bytes()[byte..];
        let parsed =
            self.parser
                .parse_with_options(&mut input, previous_tree.as_ref(), Some(options));
        let Some(tree) = parsed else {
            self.parser.reset();
            return None;
        };
        if cancelled() {
            return None;
        }
        let projection = project_tree(source, &tree, cancelled)?;
        if cancelled() {
            return None;
        }
        self.previous = Some((source.to_owned(), tree));
        Some(projection)
    }
}

fn source_point(source: &str, byte: usize) -> Point {
    let previous = &source[..byte];
    let row = previous.bytes().filter(|&value| value == b'\n').count();
    let column = byte - previous.rfind('\n').map_or(0, |line| line + 1);
    Point::new(row, column)
}

/// UTF-8-safe one-change diff for Tree::edit; byte positions match Tree-sitter Point columns.
fn minimal_edit(old: &str, new: &str) -> InputEdit {
    let common = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    let mut start = common;
    while !old.is_char_boundary(start) || !new.is_char_boundary(start) {
        start -= 1;
    }
    let mut old_end = old.len();
    let mut new_end = new.len();
    while old_end > start
        && new_end > start
        && old.as_bytes()[old_end - 1] == new.as_bytes()[new_end - 1]
    {
        old_end -= 1;
        new_end -= 1;
    }
    // The identical suffix may have been counted through a multibyte code point.
    while !old.is_char_boundary(old_end) || !new.is_char_boundary(new_end) {
        old_end += 1;
        new_end += 1;
    }
    InputEdit {
        start_byte: start,
        old_end_byte: old_end,
        new_end_byte: new_end,
        start_position: source_point(old, start),
        old_end_position: source_point(old, old_end),
        new_end_position: source_point(new, new_end),
    }
}

/// Every successful mapping links the SAME AST token occurrence in the original and projected texts.
/// Unsupported syntax is preserved. This does not resolve symbols or prove Java/Rust equivalence.
pub fn project(source: &str) -> Projection {
    ProjectionEngine::new()
        .ok()
        .and_then(|mut engine| engine.project(source, &|| false))
        .unwrap_or_else(|| Projection {
            java: source.to_owned(),
            rust_line_starts: line_starts(source),
            java_line_starts: line_starts(source),
            warning: true,
            ..Default::default()
        })
}

// Build compact per-line indexes while keeping extreme cross-line spans separate.
fn add_span_to_lookup(
    lookup: &mut [Vec<usize>],
    starts: &[usize],
    span: ByteSpan,
    index: usize,
    wide: &mut Vec<usize>,
) {
    if span.start >= span.end || starts.is_empty() {
        return;
    }
    let first = starts
        .partition_point(|&start| start <= span.start)
        .saturating_sub(1);
    let last = starts
        .partition_point(|&start| start < span.end)
        .saturating_sub(1);
    if last.saturating_sub(first) > 256 {
        wide.push(index);
    } else {
        for line in first..=last {
            if let Some(bucket) = lookup.get_mut(line) {
                bucket.push(index);
            }
        }
    }
}

fn project_tree(source: &str, tree: &Tree, cancelled: &dyn Fn() -> bool) -> Option<Projection> {
    let mut collector = Collector {
        source,
        edits: Vec::new(),
        drafts: Vec::new(),
        seen: HashMap::new(),
        cancelled,
        visited: 0,
        insights: Vec::new(),
        structures: Vec::new(),
        folds: Vec::new(),
        rename_cache: HashMap::new(),
    };
    if !collector.walk(tree.root_node(), false) || cancelled() {
        return None;
    }
    collector.edits.sort_by_key(|edit| edit.rust.start);
    // Reject overlapping replacement spans rather than generate an incorrect offset map.
    let mut non_overlapping = Vec::new();
    for edit in collector.edits {
        if non_overlapping
            .last()
            .is_none_or(|last: &Edit| last.rust.end <= edit.rust.start)
        {
            non_overlapping.push(edit);
        }
    }
    let java = apply_edits(source, &non_overlapping);
    let mut offsets = Vec::with_capacity(non_overlapping.len() + 1);
    offsets.push(0);
    for edit in &non_overlapping {
        offsets.push(
            offsets.last().copied().unwrap_or(0) + edit.replacement.len() as isize
                - edit.rust.len() as isize,
        );
    }
    let rust_lines = line_starts(source);
    let java_lines = line_starts(&java);
    let mut source_hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut source_hasher);
    let source_hash = source_hasher.finish();
    for fold in &mut collector.folds {
        fold.java_byte = map_offset(fold.rust_byte, &non_overlapping, &offsets);
        fold.java_end_byte = map_offset(fold.rust_end_byte, &non_overlapping, &offsets);
    }
    let mut fold_starts = vec![None; rust_lines.len()];
    for (index, fold) in collector.folds.iter().enumerate() {
        if let Some(slot) = fold_starts.get_mut(fold.start_line) {
            let previous = slot.map(|old: usize| &collector.folds[old]);
            let better = previous.is_none_or(|old| {
                fold.end_line > old.end_line
                    || (fold.end_line == old.end_line
                        && old.kind == "block"
                        && fold.kind != "block")
            });
            if better {
                *slot = Some(index);
            }
        }
    }
    let mut anchors = Vec::new();
    for draft in collector.drafts {
        if cancelled() {
            return None;
        }
        if let Some(projected) = map_span(draft.rust, &non_overlapping, &offsets) {
            anchors.push(Anchor {
                rust: draft.rust,
                java: projected,
                rust_at: locate(source, &rust_lines, draft.rust.start),
                java_at: locate(&java, &java_lines, projected.start),
                symbol: source[draft.rust.start..draft.rust.end].to_owned(),
                kind: draft.kind,
                topic: draft.topic,
                explanation: draft.explanation,
            });
        }
    }
    anchors.sort_by_key(|anchor| (anchor.rust.start, anchor.rust.end));
    let mut anchors_by_line = vec![Vec::new(); rust_lines.len()];
    let mut rust_lookup_lines = vec![Vec::new(); rust_lines.len()];
    let mut java_lookup_lines = vec![Vec::new(); java_lines.len()];
    let mut rust_wide_anchors = Vec::new();
    let mut java_wide_anchors = Vec::new();
    for (index, anchor) in anchors.iter().enumerate() {
        anchors_by_line[anchor.rust_at.line].push(index);
        add_span_to_lookup(
            &mut rust_lookup_lines,
            &rust_lines,
            anchor.rust,
            index,
            &mut rust_wide_anchors,
        );
        add_span_to_lookup(
            &mut java_lookup_lines,
            &java_lines,
            anchor.java,
            index,
            &mut java_wide_anchors,
        );
    }
    Some(Projection {
        java,
        anchors,
        anchors_by_line,
        rust_lookup_lines,
        java_lookup_lines,
        rust_wide_anchors,
        java_wide_anchors,
        rust_line_starts: rust_lines,
        java_line_starts: java_lines,
        java_insights: collector.insights,
        structures: collector.structures,
        folds: collector.folds,
        fold_starts,
        source_hash,
        warning: tree.root_node().has_error(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_links(source: &str) {
        let projection = project(source);
        for (i, anchor) in projection.anchors.iter().enumerate() {
            assert_eq!(
                projection.rust_anchor(anchor.rust.start),
                Some(i),
                "original index for {:?} not precise",
                anchor
            );
            assert_eq!(
                projection.java_anchor(anchor.java.start),
                Some(i),
                "projected index for {:?} not precise",
                anchor
            );
            assert_eq!(anchor.rust_at.line, anchor.java_at.line);
            assert!(projection.anchors_by_line[anchor.rust_at.line].contains(&i));
            assert!(source.is_char_boundary(anchor.rust.start));
            assert!(projection.java.is_char_boundary(anchor.java.start));
        }
    }

    #[test]
    fn teaching_view_import_module_and_camel_names_preserve_exact_ast_links() {
        let src = r#"use std::ops::Range;
mod helper_utils {
    pub struct SomeType { first_name: String }
    impl SomeType {
        pub fn get_first_name(&self) -> &str { &self.first_name }
    }
    pub fn minimal_diff_span(old_end: usize) -> usize { old_end }
}"#;
        let p = project(src);
        assert!(p.java.contains("import std::ops::Range;"));
        assert!(p.java.contains("/*module≈package*/ helper_utils {"));
        assert!(p.java.contains("firstName: String"));
        assert!(p.java.contains("getFirstName(&self)"));
        assert!(p.java.contains("self.firstName"));
        assert!(p.java.contains("minimalDiffSpan(oldEnd"));
        assert!(p.java.contains("{ oldEnd }"));
        assert!(
            p.structures
                .iter()
                .any(|e| e.title.contains("模块 helper_utils"))
        );
        assert!(
            p.structures
                .iter()
                .any(|e| e.title.contains("class SomeType"))
        );
        assert!(
            p.structures
                .iter()
                .any(|e| e.title.contains("成员方法 getFirstName"))
        );
        assert!(
            p.structures
                .iter()
                .any(|e| e.title.contains("自由函数 minimalDiffSpan"))
        );
        assert!(p.folds.iter().any(|f| f.kind == "mod_item"));
        assert!(p.folds.iter().any(|f| f.kind == "impl_item"));
        assert_links(src);
    }

    #[test]
    fn actual_line_index_rs_screenshot_case_has_java_style_free_function_outline() {
        let source = include_str!("line_index.rs");
        let p = project(source);
        assert!(p.java.contains("import std::ops::Range;"));
        assert!(p.java.contains("/*method*/ applyLinePatch("));
        assert!(p.java.contains("var /*mutable*/ oldEnd = before.len()"));
        assert!(p.java.contains("var /*mutable*/ newEnd = after.len()"));
        assert!(
            p.structures
                .iter()
                .any(|e| e.title.contains("自由函数 minimalDiff"))
        );
        assert!(
            p.structures
                .iter()
                .any(|e| e.title.contains("自由函数 applyLinePatch"))
        );
        assert!(p.folds.iter().any(|f| f.kind == "function_item"));
        assert_links(source);
    }

    #[test]
    fn standalone_function_file_has_a_nonempty_structure_navigation() {
        let src = "use std::ops::Range;\n\npub fn minimal_diff_span(before: &str, after: &str) -> usize {\n    let mut old_end = before.len();\n    while old_end > 0 {\n        old_end -= 1;\n    }\n    old_end\n}\n";
        let p = project(src);
        assert!(p.java.starts_with("import std::ops::Range;"));
        assert!(p.java.contains("minimalDiffSpan("));
        assert!(p.java.contains("oldEnd"));
        assert!(
            p.structures
                .iter()
                .any(|e| e.title.contains("minimalDiffSpan"))
        );
        assert!(p.folds.iter().any(|f| f.kind == "while_expression"));
        assert_links(src);
    }

    #[test]
    fn lowercase_struct_type_is_pascal_in_java_projection_but_real_rust_remains_intact() {
        let src = "struct user_profile { account_id: usize }\nimpl user_profile {\n    fn get_account_id(&self) -> usize { self.account_id }\n}\n";
        let p = project(src);
        assert!(p.java.contains("class UserProfile { accountId: usize }"));
        assert!(
            p.java.contains("/*"),
            "the teaching view is explicitly conceptual"
        );
        assert!(p.java.contains("impl UserProfile {"));
        assert!(p.java.contains("getAccountId"));
        assert!(
            p.structures
                .iter()
                .any(|entry| entry.title.contains("class UserProfile"))
        );
        assert!(
            p.structures
                .iter()
                .any(|entry| entry.title.contains("UserProfile 的方法"))
        );
        assert_links(src);
    }

    #[test]
    fn never_rename_import_paths_macros_types_or_unusual_identifiers() {
        let src = r#"use project_mod::snake_case;
mod internal_mod;
fn compute_value() {
    custom_macro!(snake_case);
    let SOME_CONSTANT = 1;
    let __internal = 2;
    let a = SomeCustomType::make_value();
}"#;
        let p = project(src);
        assert!(p.java.contains("import project_mod::snake_case"));
        assert!(p.java.contains("/*module≈package*/ internal_mod;"));
        assert!(p.java.contains("custom_macro!(snake_case)"));
        assert!(p.java.contains("SOME_CONSTANT"));
        assert!(p.java.contains("__internal"));
        assert!(p.java.contains("SomeCustomType::makeValue()"));
        assert_links(src);
    }

    #[test]
    fn declarations_and_fields_are_linked_exactly() {
        let src = "struct User { name: String, count: u32 }\nimpl User { fn get(&self) -> &str { &self.name } }";
        let p = project(src);
        assert!(p.java.contains("class User"));
        assert!(p.java.contains("/*method*/ get"));
        for symbol in ["User", "name", "count", "get"] {
            assert!(
                p.anchors.iter().any(|a| a.symbol == symbol),
                "missing {symbol}"
            );
        }
        assert_links(src);
    }

    #[test]
    fn multiple_edits_same_line_have_precise_reverse_mapping() {
        let src = "fn f() { let a: Option<Vec<i32>> = None; let b = 1; }";
        let p = project(src);
        assert!(
            p.java
                .contains("/*method*/ f() { var a: Option<Vec<i32>> = None; var b = 1; }")
        );
        assert_links(src);
        let a = p.anchors.iter().find(|a| a.symbol == "Option").unwrap();
        assert_eq!(&p.java[a.java.start..a.java.end], "Option");
    }

    #[test]
    fn unicode_and_crlf_offsets_are_valid() {
        let src = "// 学习 Rust 🦀\r\nstruct 用户 { 名字: String }\r\nfn main() { let 问候 = \"你好\"; }\r\n";
        let p = project(src);
        assert!(p.java.contains("class 用户"));
        assert_links(src);
        let user = p.anchors.iter().find(|a| a.symbol == "用户").unwrap();
        assert_eq!(user.rust_at.line, 1);
        assert_eq!(user.java_at.line, 1);
    }

    #[test]
    fn preserves_unsupported_macros_lifetimes_and_syntax_errors() {
        let src = "fn demo<'a>(x: &'a str) { println!(\"ok\"); }";
        let p = project(src);
        assert!(p.java.contains("println!"));
        assert!(p.java.contains("'a"));
        assert_links(src);
        assert!(project("fn broken( {").warning);
        let macro_source = "custom! { fn example() { let value: Option<i32> = None; } }";
        assert_eq!(project(macro_source).java, macro_source);
    }

    #[test]
    fn not_a_compiler_or_symbol_resolver() {
        let src = "struct Option { x: i32 }\nfn main() { let y: Option = Option { x: 1 }; }";
        let p = project(src);
        assert!(
            p.anchors
                .iter()
                .any(|a| a.topic == "Option<T> ↔ Optional<T>")
        );
        // Familiar type names are not rewritten without semantic type resolution.
        assert!(p.java.contains("class Option {"));
        assert!(p.java.contains("var y: Option"));
        assert!(
            p.anchors
                .iter()
                .any(|a| a.explanation.contains("未解析到类型身份"))
        );
    }

    #[test]
    fn custom_type_names_are_never_rewritten_by_name_only() {
        let source =
            "struct Option { value: i32 }\nstruct Vec { count: usize }\nfn f(x: Option, y: Vec) {}";
        let projection = project(source);
        assert!(projection.java.contains("class Option {"));
        assert!(projection.java.contains("class Vec {"));
        assert!(!projection.java.contains("Optional"));
        assert!(!projection.java.contains("ArrayList"));
        assert_links(source);
    }

    #[test]
    fn enum_and_iterator_concepts_are_linked_without_guessing_types() {
        let src = "enum Event { Count(u32), Stop }\nfn main() { let xs = vec![1]; xs.iter().map(|x| x + 1).collect::<Vec<_>>(); }";
        let projection = project(src);
        assert!(
            projection
                .anchors
                .iter()
                .any(|anchor| anchor.topic == "Rust enum ↔ tagged union")
        );
        assert!(
            projection
                .anchors
                .iter()
                .any(|anchor| anchor.topic == "Iterator traversal")
        );
        assert!(
            projection
                .anchors
                .iter()
                .any(|anchor| anchor.topic == "Iterator pipeline")
        );
        assert_links(src);
    }

    #[test]
    fn borrowing_and_move_closures_are_precisely_anchored() {
        let src = "fn main() { let text = String::from(\"x\"); let borrowed = &text; let task = move || text.len(); }";
        let projection = project(src);
        assert!(projection.anchors.iter().any(|anchor| {
            anchor.topic == "Reference and borrowing"
                && &src[anchor.rust.start..anchor.rust.end] == "&"
        }));
        assert!(projection.anchors.iter().any(|anchor| {
            anchor.topic == "move closure capture"
                && &src[anchor.rust.start..anchor.rust.end] == "move"
        }));
        assert_links(src);
    }

    #[test]
    fn source_maps_do_not_claim_whitespace_is_symbol() {
        let p = project("struct A { x: String }");
        assert!(p.rust_anchor(0).is_some());
        assert_eq!(p.rust_anchor(6), None); // space between keyword and name
        assert_eq!(char_to_byte("中a", 1), 3);
        assert_eq!(char_to_byte("中a", 2), 4);
    }

    #[test]
    fn incremental_edit_matches_full_parse_for_unicode_and_multiline() {
        let mut engine = ProjectionEngine::new().unwrap();
        let originals = [
            "struct User { name: String }",
            "struct 用户 { name: String }",
            "struct 用户 {\n name: Option<String>,\n visit: u32\n }",
            "struct 用户 {\n name: Option<String>,\n visits: u32\n }",
            "",
            "fn main() { let hello = \"你好🦀\"; }",
        ];
        for source in originals {
            let incremental = engine.project(source, &|| false).unwrap();
            let full = project(source);
            assert_eq!(incremental.java, full.java);
            assert_eq!(incremental.anchors.len(), full.anchors.len());
            assert_eq!(incremental.rust_line_starts, full.rust_line_starts);
            assert_eq!(incremental.java_line_starts, full.java_line_starts);
        }
    }

    #[test]
    fn structural_java_lens_explains_impl_and_trait() {
        let source = "struct User { name: String }\ntrait Greeter {}\nimpl Greeter for User {}\nimpl User {}";
        let p = project(source);
        assert!(
            p.java_insights
                .iter()
                .any(|i| i.headline.contains("class User"))
        );
        assert!(
            p.java_insights
                .iter()
                .any(|i| i.headline.contains("interface Greeter"))
        );
        assert!(
            p.java_insights
                .iter()
                .any(|i| i.headline.contains("User implements Greeter"))
        );
        assert!(
            p.java_insights
                .iter()
                .any(|i| i.headline.contains("User 的方法"))
        );
    }

    #[test]
    fn cancelled_projection_is_not_published() {
        let mut engine = ProjectionEngine::new().unwrap();
        let ok = engine.project("struct Original {}", &|| false).unwrap();
        assert_eq!(ok.java, "class Original {}");
        assert!(engine.project("struct Other {}", &|| true).is_none());
        assert_eq!(
            engine
                .project("struct Original {}", &|| false)
                .unwrap()
                .java,
            ok.java
        );
    }

    #[test]
    fn empty_file_and_trailing_newline() {
        let p = project("");
        assert_eq!(p.java, "");
        assert!(p.anchors.is_empty());
        assert_eq!(project("fn main() {}\n").java.matches('\n').count(), 1);
    }
    #[test]
    fn indexed_span_lookup_matches_exact_reference_for_all_bytes() {
        let source =
            "struct User { name: String }\nimpl User { fn greet(&self) -> &str { &self.name } }\n";
        let projection = project(source);
        for (bytes, view) in [(source.len(), false), (projection.java.len(), true)] {
            for position in 0..bytes {
                let baseline = projection
                    .anchors
                    .iter()
                    .enumerate()
                    .filter(|(_, anchor)| {
                        if view {
                            anchor.java.contains(position)
                        } else {
                            anchor.rust.contains(position)
                        }
                    })
                    .min_by_key(|(_, anchor)| {
                        if view {
                            anchor.java.len()
                        } else {
                            anchor.rust.len()
                        }
                    })
                    .map(|(index, _)| index);
                let actual = if view {
                    projection.java_anchor(position)
                } else {
                    projection.rust_anchor(position)
                };
                assert_eq!(actual, baseline, "view={view} byte={position}");
            }
        }
    }
}
