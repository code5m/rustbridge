use tree_sitter::{Node, Parser};
#[derive(Clone, Debug)]
pub struct Note {
    pub line: usize,
    pub topic: &'static str,
    pub detail: &'static str,
}
#[derive(Clone, Debug, Default)]
pub struct Projection {
    pub java: String,
    pub notes: Vec<Note>,
    pub warning: bool,
}
fn visit(node: Node<'_>, src: &str, lines: &mut [String], notes: &mut Vec<Note>) {
    let row = node.start_position().row;
    let (kind, topic, detail) = match node.kind() {
        "struct_item" => (
            "struct",
            "struct → class",
            "Rust separates struct fields from impl methods. No class inheritance.",
        ),
        "trait_item" => (
            "trait",
            "trait → interface",
            "Traits express behavior and support default methods.",
        ),
        "impl_item" => (
            "impl",
            "impl → methods",
            "An impl block attaches methods or implements a trait.",
        ),
        "function_item" => (
            "fn",
            "fn → method",
            "&self borrows the receiver; self transfers ownership.",
        ),
        "let_declaration" => (
            "let",
            "let → local variable",
            "Bindings are immutable unless mut; ownership moves by default for non-Copy values.",
        ),
        "reference_type" => (
            "borrow",
            "&T → borrowing",
            "Borrowing is checked at compile time and is not the same as a Java reference.",
        ),
        "macro_invocation" => (
            "macro",
            "Rust macro",
            "Macros are preserved as Rust; automatic Java conversion would be misleading.",
        ),
        "generic_type" => {
            let t = node.utf8_text(src.as_bytes()).unwrap_or("");
            if t.starts_with("Option<") {
                (
                    "Option",
                    "Option<T> → Optional<T>",
                    "Some/None force missing values to be handled.",
                )
            } else if t.starts_with("Result<") {
                (
                    "Result",
                    "Result<T,E> → explicit error",
                    "Result expresses fallible operations as values; ? propagates errors.",
                )
            } else if t.starts_with("Vec<") {
                (
                    "Vec",
                    "Vec<T> → ArrayList<T>",
                    "Growable, owned, contiguous collection.",
                )
            } else if t.starts_with("HashMap<") {
                (
                    "HashMap",
                    "HashMap<K,V> → Map<K,V>",
                    "Map insertion owns values; lookups commonly borrow.",
                )
            } else {
                ("", "", "")
            }
        }
        _ => ("", "", ""),
    };
    if !topic.is_empty() && !notes.iter().any(|n| n.line == row && n.topic == topic) {
        notes.push(Note {
            line: row,
            topic,
            detail,
        });
    }
    if row < lines.len() {
        match kind {
            "struct" => {
                if let Some(name) = node.child_by_field_name("name") {
                    if let Ok(name) = name.utf8_text(src.as_bytes()) {
                        lines[row] =
                            format!("class {name} {{ // Rust struct; educational approximation");
                    }
                }
            }
            "trait" => {
                if let Some(name) = node.child_by_field_name("name") {
                    if let Ok(name) = name.utf8_text(src.as_bytes()) {
                        lines[row] =
                            format!("interface {name} {{ // Rust trait; educational approximation");
                    }
                }
            }
            "impl" => lines[row] = format!("// impl (Rust methods): {}", lines[row].trim()),
            "fn" => lines[row] = lines[row].replacen("fn ", "/* method */ ", 1),
            "let" => {
                lines[row] = lines[row]
                    .replacen("let mut ", "/* mutable */ var ", 1)
                    .replacen("let ", "var ", 1)
            }
            _ => {}
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit(child, src, lines, notes);
    }
}
/// A source-aligned educational view, NEVER a compilable Java translation.
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
    let mut lines: Vec<String> = source.lines().map(str::to_owned).collect();
    let mut notes = Vec::new();
    visit(tree.root_node(), source, &mut lines, &mut notes);
    notes.sort_by_key(|n| (n.line, n.topic));
    Projection {
        java: lines.join("\n"),
        notes,
        warning: tree.root_node().has_error(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn struct_maps_to_class() {
        assert!(
            project("struct User { name: String }")
                .java
                .contains("class User")
        );
    }
    #[test]
    fn trait_maps_to_interface() {
        assert!(
            project("trait Greet { fn hi(&self); }")
                .java
                .contains("interface Greet")
        );
    }
    #[test]
    fn preserves_macros() {
        assert!(
            project("fn main() { println!(\"x\"); }")
                .java
                .contains("println!")
        );
    }
    #[test]
    fn incomplete_source_warns() {
        assert!(project("fn foo( {").warning);
    }
}
