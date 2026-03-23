use tree_sitter::Node;

use crate::model::Import;
use crate::model::declarations::{DeclKind, Declaration, Visibility};

use super::DeclExtractor;

pub struct GoExtractor;

impl DeclExtractor for GoExtractor {
    fn extract(&self, root: Node<'_>, source: &str) -> (Vec<Import>, Vec<Declaration>) {
        let mut imports = Vec::new();
        let mut declarations = Vec::new();

        for i in 0..root.child_count() {
            let Some(child) = root.child(i) else {
                continue;
            };
            match child.kind() {
                "import_declaration" => {
                    if let Some(import) = extract_import(child, source) {
                        imports.push(import);
                    }
                }
                "function_declaration" => {
                    if let Some(decl) = extract_function(child, source) {
                        declarations.push(decl);
                    }
                }
                "method_declaration" => {
                    if let Some(decl) = extract_method(child, source) {
                        declarations.push(decl);
                    }
                }
                "type_declaration" => {
                    let mut decls = extract_type_declaration(child, source);
                    declarations.append(&mut decls);
                }
                "const_declaration" => {
                    let mut decls = extract_const_declaration(child, source);
                    declarations.append(&mut decls);
                }
                "var_declaration" => {
                    let mut decls = extract_var_declaration(child, source);
                    declarations.append(&mut decls);
                }
                "package_clause" => {
                    // Recognized but not included in declarations
                }
                _ => {}
            }
        }

        (imports, declarations)
    }
}

fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.start_byte()..node.end_byte()]
}

fn extract_visibility(name: &str) -> Visibility {
    if name.starts_with(|c: char| c.is_ascii_uppercase()) {
        Visibility::Public
    } else {
        Visibility::Private
    }
}

fn extract_doc_comment(node: Node<'_>, source: &str) -> Option<String> {
    let mut comments = Vec::new();
    let mut prev = node.prev_sibling();

    while let Some(sibling) = prev {
        match sibling.kind() {
            "comment" => {
                let text = node_text(sibling, source);
                if let Some(line_comment) = text.strip_prefix("//") {
                    comments.push(line_comment.trim().to_string());
                } else if text.starts_with("/*") {
                    let cleaned = text
                        .trim_start_matches("/*")
                        .trim_end_matches("*/")
                        .trim()
                        .to_string();
                    comments.push(cleaned);
                    break;
                }
            }
            _ => break,
        }
        prev = sibling.prev_sibling();
    }

    if comments.is_empty() {
        None
    } else {
        comments.reverse();
        Some(comments.join(" "))
    }
}

fn extract_signature(node: Node<'_>, source: &str) -> String {
    let text = node_text(node, source);
    // Take text up to the opening brace
    let end = text.find('{').unwrap_or(text.len());
    let sig = text[..end].trim();
    sig.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn extract_import(node: Node<'_>, source: &str) -> Option<Import> {
    let text = node_text(node, source).trim().to_string();
    Some(Import { text })
}

fn extract_function(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let visibility = extract_visibility(&name);
    let doc_comment = extract_doc_comment(node, source);
    let signature = extract_signature(node, source);
    let line = node.start_position().row + 1;

    Some(Declaration {
        kind: DeclKind::Function,
        name,
        signature,
        visibility,
        line,
        doc_comment,
        children: Vec::new(),
    })
}

fn extract_receiver_type(node: Node<'_>, source: &str) -> Option<String> {
    let receiver = node.child_by_field_name("receiver")?;
    // The receiver is a parameter_list like "(s *Server)" or "(s Server)"
    // We want just the type name, stripping the pointer star and variable name.
    let text = node_text(receiver, source);
    let inner = text.trim_start_matches('(').trim_end_matches(')').trim();
    // Could be "s *Server" or "s Server" or "*Server"
    let parts: Vec<&str> = inner.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }
    // Take the last part which is the type, and strip the pointer star
    let type_name = parts.last()?.trim_start_matches('*');
    Some(type_name.to_string())
}

fn extract_method(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let method_name = node_text(name_node, source).to_string();
    let receiver_type = extract_receiver_type(node, source);

    let name = if let Some(ref recv) = receiver_type {
        format!("{}.{}", recv, method_name)
    } else {
        method_name.clone()
    };

    let visibility = extract_visibility(&method_name);
    let doc_comment = extract_doc_comment(node, source);
    let signature = extract_signature(node, source);
    let line = node.start_position().row + 1;

    Some(Declaration {
        kind: DeclKind::Method,
        name,
        signature,
        visibility,
        line,
        doc_comment,
        children: Vec::new(),
    })
}

fn extract_type_declaration(node: Node<'_>, source: &str) -> Vec<Declaration> {
    let mut declarations = Vec::new();
    let doc_comment = extract_doc_comment(node, source);

    for i in 0..node.child_count() {
        let Some(child) = node.child(i) else {
            continue;
        };
        if child.kind() == "type_spec" {
            if let Some(decl) = extract_type_spec(child, source, &doc_comment) {
                declarations.push(decl);
            }
        }
    }

    declarations
}

fn extract_type_spec(
    node: Node<'_>,
    source: &str,
    parent_doc: &Option<String>,
) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let visibility = extract_visibility(&name);
    let line = node.start_position().row + 1;

    // Try to get a doc comment from the type_spec itself; fall back to parent
    let doc_comment = extract_doc_comment(node, source).or_else(|| parent_doc.clone());

    let type_node = node.child_by_field_name("type")?;

    match type_node.kind() {
        "struct_type" => {
            let signature = format!("type {} struct", name);
            let children = extract_struct_fields(type_node, source);

            Some(Declaration {
                kind: DeclKind::Struct,
                name,
                signature,
                visibility,
                line,
                doc_comment,
                children,
            })
        }
        "interface_type" => {
            let signature = format!("type {} interface", name);
            let children = extract_interface_methods(type_node, source);

            Some(Declaration {
                kind: DeclKind::Trait, // interfaces map to Trait
                name,
                signature,
                visibility,
                line,
                doc_comment,
                children,
            })
        }
        _ => {
            // Other type declarations (type aliases, etc.)
            let type_text = node_text(type_node, source);
            let signature = format!("type {} {}", name, type_text)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");

            Some(Declaration {
                kind: DeclKind::TypeAlias,
                name,
                signature,
                visibility,
                line,
                doc_comment,
                children: Vec::new(),
            })
        }
    }
}

fn extract_struct_fields(node: Node<'_>, source: &str) -> Vec<Declaration> {
    let mut fields = Vec::new();

    // struct_type has a field_declaration_list body
    for i in 0..node.child_count() {
        let Some(child) = node.child(i) else {
            continue;
        };
        if child.kind() == "field_declaration_list" {
            for j in 0..child.child_count() {
                let Some(field) = child.child(j) else {
                    continue;
                };
                if field.kind() == "field_declaration" {
                    if let Some(decl) = extract_struct_field(field, source) {
                        fields.push(decl);
                    }
                }
            }
        }
    }

    fields
}

fn extract_struct_field(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let visibility = extract_visibility(&name);
    let line = node.start_position().row + 1;

    let type_text = node
        .child_by_field_name("type")
        .map(|t| node_text(t, source))
        .unwrap_or("");

    let signature = format!("{} {}", name, type_text);

    Some(Declaration {
        kind: DeclKind::Field,
        name,
        signature,
        visibility,
        line,
        doc_comment: None,
        children: Vec::new(),
    })
}

fn extract_interface_methods(node: Node<'_>, source: &str) -> Vec<Declaration> {
    let mut methods = Vec::new();

    for i in 0..node.child_count() {
        let Some(child) = node.child(i) else {
            continue;
        };
        if child.kind() == "method_spec" {
            if let Some(decl) = extract_method_spec(child, source) {
                methods.push(decl);
            }
        }
    }

    methods
}

fn extract_method_spec(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let visibility = extract_visibility(&name);
    let line = node.start_position().row + 1;
    let signature = node_text(node, source)
        .trim()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    Some(Declaration {
        kind: DeclKind::Method,
        name,
        signature,
        visibility,
        line,
        doc_comment: None,
        children: Vec::new(),
    })
}

fn extract_const_declaration(node: Node<'_>, source: &str) -> Vec<Declaration> {
    let mut declarations = Vec::new();
    let doc_comment = extract_doc_comment(node, source);

    for i in 0..node.child_count() {
        let Some(child) = node.child(i) else {
            continue;
        };
        if child.kind() == "const_spec" {
            if let Some(decl) = extract_const_spec(child, source, &doc_comment) {
                declarations.push(decl);
            }
        }
    }

    declarations
}

fn extract_const_spec(
    node: Node<'_>,
    source: &str,
    parent_doc: &Option<String>,
) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let visibility = extract_visibility(&name);
    let line = node.start_position().row + 1;
    let doc_comment = extract_doc_comment(node, source).or_else(|| parent_doc.clone());
    let signature = node_text(node, source)
        .trim()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    Some(Declaration {
        kind: DeclKind::Constant,
        name,
        signature,
        visibility,
        line,
        doc_comment,
        children: Vec::new(),
    })
}

fn extract_var_declaration(node: Node<'_>, source: &str) -> Vec<Declaration> {
    let mut declarations = Vec::new();
    let doc_comment = extract_doc_comment(node, source);

    for i in 0..node.child_count() {
        let Some(child) = node.child(i) else {
            continue;
        };
        if child.kind() == "var_spec" {
            if let Some(decl) = extract_var_spec(child, source, &doc_comment) {
                declarations.push(decl);
            }
        }
    }

    declarations
}

fn extract_var_spec(
    node: Node<'_>,
    source: &str,
    parent_doc: &Option<String>,
) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let visibility = extract_visibility(&name);
    let line = node.start_position().row + 1;
    let doc_comment = extract_doc_comment(node, source).or_else(|| parent_doc.clone());
    let signature = node_text(node, source)
        .trim()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    Some(Declaration {
        kind: DeclKind::Static, // var maps to Static
        name,
        signature,
        visibility,
        line,
        doc_comment,
        children: Vec::new(),
    })
}
