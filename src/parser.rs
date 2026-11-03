//! Tree-sitter-backed, occurrence-level source maps for the Java reading projection.
//! A projection is a teaching aid, not a Java transpiler or a name resolver.

use std::collections::HashMap;
use tree_sitter::{Node, Parser};

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

#[derive(Clone, Debug, Default)]
pub struct Projection {
    pub java: String,
    pub anchors: Vec<Anchor>,
    /// All linked occurrences grouped by their original line; rewrites preserve line breaks.
    pub anchors_by_line: Vec<Vec<usize>>,
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
        self.anchors
            .iter()
            .enumerate()
            .filter(|(_, anchor)| {
                if in_java {
                    anchor.java.contains(byte)
                } else {
                    anchor.rust.contains(byte)
                }
            })
            .min_by_key(|(_, anchor)| {
                if in_java {
                    anchor.java.len()
                } else {
                    anchor.rust.len()
                }
            })
            .map(|(idx, _)| idx)
    }
}

#[derive(Clone)]
struct Edit {
    rust: ByteSpan,
    replacement: &'static str,
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
}

impl Collector<'_> {
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
                    replacement,
                });
            }
        }
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

    fn walk(&mut self, node: Node<'_>) {
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
                self.named(node, "type", "Rust enum",
                    "Rust enums can carry data in variants. Java enums are not a complete equivalent.");
            }
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
                    "Rust let binds a value; bindings are immutable unless mut. Non-Copy values can move ownership.");
            }
            "reference_type" => self.keyword(node, "&", None, "Reference and borrowing",
                "A reference borrows a value; the borrow checker prevents invalid lifetimes and conflicting mutable access."),
            "macro_invocation" => {
                // Intentionally keep macro syntax unchanged: expansion needs compiler semantics.
            }
            _ => {}
        }

        if node.child_count() == 0 {
            self.leaf(node);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk(child);
        }
    }

    fn leaf(&mut self, node: Node<'_>) {
        let text = node.utf8_text(self.source.as_bytes()).unwrap_or("");
        match node.kind() {
            "type_identifier" => {
                let (topic, detail, replacement) = match text {
                    "Option" => ("Option<T> ↔ Optional<T>",
                        "Convention-based reading only: Option has Some/None, not null. A type resolver is needed to confirm this identifier refers to std::option::Option.", Some("Optional")),
                    "Vec" => ("Vec<T> ↔ ArrayList<T>",
                        "Convention-based reading only: Vec is an owned contiguous growable array, not a Java object reference.", Some("ArrayList")),
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
                self.add(node, "field",
                    if is_decl { "Field declaration" } else { "Field or method access" },
                    if is_decl {
                        "This is the declared field name. The paired Rust span is its exact AST occurrence."
                    } else {
                        "This is a field or method occurrence, not necessarily the declaration. Cross-file name resolution is not implemented."
                    }, 1);
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
        result.push_str(edit.replacement);
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

fn map_span(span: ByteSpan, edits: &[Edit]) -> Option<ByteSpan> {
    let mut delta = 0_isize;
    for edit in edits {
        if edit.rust.end <= span.start {
            delta += edit.replacement.len() as isize - edit.rust.len() as isize;
        } else if edit.rust.start >= span.end {
            break;
        } else if edit.rust == span {
            let start = shifted(span.start, delta);
            return Some(ByteSpan {
                start,
                end: start + edit.replacement.len(),
            });
        } else {
            // Never present a partial or overlapping rewrite as a precise source map.
            return None;
        }
    }
    Some(ByteSpan {
        start: shifted(span.start, delta),
        end: shifted(span.end, delta),
    })
}

fn line_starts(source: &str) -> Vec<usize> {
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

/// Every successful mapping links the SAME AST token occurrence in the original and projected texts.
/// Unsupported syntax is preserved. This does not resolve symbols or prove Java/Rust equivalence.
pub fn project(source: &str) -> Projection {
    let mut parser = Parser::new();
    if parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .is_err()
    {
        return Projection {
            java: source.into(),
            warning: true,
            ..Default::default()
        };
    }
    let Some(tree) = parser.parse(source, None) else {
        return Projection {
            java: source.into(),
            warning: true,
            ..Default::default()
        };
    };
    let mut collector = Collector {
        source,
        edits: Vec::new(),
        drafts: Vec::new(),
        seen: HashMap::new(),
    };
    collector.walk(tree.root_node());
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
    let rust_lines = line_starts(source);
    let java_lines = line_starts(&java);
    let mut anchors = Vec::new();
    for draft in collector.drafts {
        if let Some(projected) = map_span(draft.rust, &non_overlapping) {
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
    for (index, anchor) in anchors.iter().enumerate() {
        anchors_by_line[anchor.rust_at.line].push(index);
    }
    Projection {
        java,
        anchors,
        anchors_by_line,
        warning: tree.root_node().has_error(),
    }
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
                .contains("/*method*/ f() { var a: Optional<ArrayList<i32>> = None; var b = 1; }")
        );
        assert_links(src);
        let a = p.anchors.iter().find(|a| a.symbol == "Option").unwrap();
        assert_eq!(&p.java[a.java.start..a.java.end], "Optional");
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
        // A type resolver is not present, so even familiar names are explicitly marked convention-based.
        assert!(
            p.anchors
                .iter()
                .any(|a| a.explanation.contains("type resolver"))
        );
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
    fn empty_file_and_trailing_newline() {
        let p = project("");
        assert_eq!(p.java, "");
        assert!(p.anchors.is_empty());
        assert_eq!(project("fn main() {}\n").java.matches('\n').count(), 1);
    }
}
