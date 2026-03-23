use tree_sitter::Node;

use crate::model::Import;
use crate::model::declarations::{DeclKind, Declaration, Visibility};

use super::DeclExtractor;

pub struct TypeScriptExtractor;

impl DeclExtractor for TypeScriptExtractor {
    fn extract(&self, root: Node<'_>, source: &str) -> (Vec<Import>, Vec<Declaration>) {
        let mut imports = Vec::new();
        let mut declarations = Vec::new();

        for i in 0..root.child_count() {
            let Some(child) = root.child(i) else {
                continue;
            };
            process_top_level_node(child, source, &mut imports, &mut declarations, false);
        }

        (imports, declarations)
    }
}

fn process_top_level_node(
    node: Node<'_>,
    source: &str,
    imports: &mut Vec<Import>,
    declarations: &mut Vec<Declaration>,
    is_exported: bool,
) {
    match node.kind() {
        "import_statement" => {
            if let Some(import) = extract_import(node, source) {
                imports.push(import);
            }
        }
        "export_statement" => {
            // An export_statement wraps a declaration; extract the inner declaration
            // and mark it as Public.
            for i in 0..node.child_count() {
                let Some(child) = node.child(i) else {
                    continue;
                };
                match child.kind() {
                    "function_declaration"
                    | "class_declaration"
                    | "abstract_class_declaration"
                    | "interface_declaration"
                    | "type_alias_declaration"
                    | "enum_declaration"
                    | "lexical_declaration" => {
                        process_top_level_node(child, source, imports, declarations, true);
                    }
                    _ => {}
                }
            }
        }
        "function_declaration" => {
            if let Some(mut decl) = extract_function(node, source) {
                if is_exported {
                    decl.visibility = Visibility::Public;
                }
                declarations.push(decl);
            }
        }
        "class_declaration" | "abstract_class_declaration" => {
            if let Some(mut decl) = extract_class(node, source) {
                if is_exported {
                    decl.visibility = Visibility::Public;
                }
                declarations.push(decl);
            }
        }
        "interface_declaration" => {
            if let Some(mut decl) = extract_interface(node, source) {
                if is_exported {
                    decl.visibility = Visibility::Public;
                }
                declarations.push(decl);
            }
        }
        "type_alias_declaration" => {
            if let Some(mut decl) = extract_type_alias(node, source) {
                if is_exported {
                    decl.visibility = Visibility::Public;
                }
                declarations.push(decl);
            }
        }
        "enum_declaration" => {
            if let Some(mut decl) = extract_enum(node, source) {
                if is_exported {
                    decl.visibility = Visibility::Public;
                }
                declarations.push(decl);
            }
        }
        "lexical_declaration" => {
            for decl in extract_lexical_declaration(node, source) {
                let mut decl = decl;
                if is_exported {
                    decl.visibility = Visibility::Public;
                }
                declarations.push(decl);
            }
        }
        _ => {}
    }
}

fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.start_byte()..node.end_byte()]
}

fn extract_doc_comment(node: Node<'_>, source: &str) -> Option<String> {
    let mut prev = node.prev_sibling();

    while let Some(sibling) = prev {
        match sibling.kind() {
            "comment" => {
                let text = node_text(sibling, source);
                if text.starts_with("/**") {
                    let cleaned = text
                        .trim_start_matches("/**")
                        .trim_end_matches("*/")
                        .trim()
                        .to_string();
                    return Some(cleaned);
                }
                return None;
            }
            _ => break,
        }
    }

    // Also check if the node's parent is an export_statement and the
    // doc comment is before the export_statement.
    if let Some(parent) = node.parent() {
        if parent.kind() == "export_statement" {
            prev = parent.prev_sibling();
            while let Some(sibling) = prev {
                match sibling.kind() {
                    "comment" => {
                        let text = node_text(sibling, source);
                        if text.starts_with("/**") {
                            let cleaned = text
                                .trim_start_matches("/**")
                                .trim_end_matches("*/")
                                .trim()
                                .to_string();
                            return Some(cleaned);
                        }
                        return None;
                    }
                    _ => break,
                }
            }
        }
    }

    None
}

fn extract_signature(node: Node<'_>, source: &str) -> String {
    let text = node_text(node, source);
    let end = text
        .find('{')
        .or_else(|| text.find(';'))
        .unwrap_or(text.len());
    let sig = text[..end].trim();
    sig.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn extract_import(node: Node<'_>, source: &str) -> Option<Import> {
    let text = node_text(node, source).trim();
    let clean = text.trim_end_matches(';').trim();
    Some(Import {
        text: clean.to_string(),
    })
}

fn extract_function(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let doc_comment = extract_doc_comment(node, source);
    let signature = extract_signature(node, source);
    let line = node.start_position().row + 1;

    Some(Declaration {
        kind: DeclKind::Function,
        name,
        signature,
        visibility: Visibility::Private,
        line,
        doc_comment,
        children: Vec::new(),
    })
}

fn extract_class(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let doc_comment = extract_doc_comment(node, source);
    let signature = extract_signature(node, source);
    let line = node.start_position().row + 1;

    let mut children = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        for i in 0..body.child_count() {
            let Some(child) = body.child(i) else {
                continue;
            };
            match child.kind() {
                "method_definition" => {
                    if let Some(method) = extract_method(child, source) {
                        children.push(method);
                    }
                }
                "public_field_definition" => {
                    if let Some(field) = extract_class_field(child, source) {
                        children.push(field);
                    }
                }
                _ => {}
            }
        }
    }

    Some(Declaration {
        kind: DeclKind::Struct,
        name,
        signature,
        visibility: Visibility::Private,
        line,
        doc_comment,
        children,
    })
}

fn extract_method(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let visibility = extract_member_visibility(node, source);
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

fn extract_class_field(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let visibility = extract_member_visibility(node, source);
    let line = node.start_position().row + 1;

    let type_text = node
        .child_by_field_name("type")
        .map(|t| node_text(t, source))
        .unwrap_or("");

    // type_annotation includes the ": " prefix, strip it
    let type_text = type_text.trim_start_matches(':').trim();

    let signature = if type_text.is_empty() {
        name.clone()
    } else {
        format!("{}: {}", name, type_text)
    };

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

/// Check for accessibility_modifier (public/private/protected) on class members.
fn extract_member_visibility(node: Node<'_>, source: &str) -> Visibility {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "accessibility_modifier" {
                let text = node_text(child, source);
                match text {
                    "public" => return Visibility::Public,
                    "private" | "protected" => return Visibility::Private,
                    _ => {}
                }
            }
        }
    }
    // Default for class members: Public (they are accessible by default in JS/TS)
    Visibility::Public
}

fn extract_interface(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let doc_comment = extract_doc_comment(node, source);
    let signature = extract_signature(node, source);
    let line = node.start_position().row + 1;

    let mut children = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        for i in 0..body.child_count() {
            let Some(child) = body.child(i) else {
                continue;
            };
            match child.kind() {
                "method_signature" | "function_signature" => {
                    if let Some(method) = extract_interface_method(child, source) {
                        children.push(method);
                    }
                }
                "property_signature" => {
                    if let Some(field) = extract_property_signature(child, source) {
                        children.push(field);
                    }
                }
                _ => {}
            }
        }
    }

    Some(Declaration {
        kind: DeclKind::Trait,
        name,
        signature,
        visibility: Visibility::Private,
        line,
        doc_comment,
        children,
    })
}

fn extract_interface_method(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let line = node.start_position().row + 1;
    let text = node_text(node, source).trim();
    let signature = text
        .trim_end_matches(';')
        .trim_end_matches(',')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    Some(Declaration {
        kind: DeclKind::Method,
        name,
        signature,
        visibility: Visibility::Public,
        line,
        doc_comment: None,
        children: Vec::new(),
    })
}

fn extract_property_signature(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let line = node.start_position().row + 1;
    let text = node_text(node, source).trim();
    let signature = text
        .trim_end_matches(';')
        .trim_end_matches(',')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    Some(Declaration {
        kind: DeclKind::Field,
        name,
        signature,
        visibility: Visibility::Public,
        line,
        doc_comment: None,
        children: Vec::new(),
    })
}

fn extract_type_alias(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let doc_comment = extract_doc_comment(node, source);
    let line = node.start_position().row + 1;
    let signature = extract_signature(node, source);

    Some(Declaration {
        kind: DeclKind::TypeAlias,
        name,
        signature,
        visibility: Visibility::Private,
        line,
        doc_comment,
        children: Vec::new(),
    })
}

fn extract_enum(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let doc_comment = extract_doc_comment(node, source);
    let signature = extract_signature(node, source);
    let line = node.start_position().row + 1;

    let mut variants = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        for i in 0..body.child_count() {
            let Some(child) = body.child(i) else {
                continue;
            };
            // enum_assignment is the node type for enum members in tree-sitter-typescript
            if child.kind() == "enum_assignment" || child.kind() == "property_identifier" {
                if let Some(variant) = extract_enum_member(child, source) {
                    variants.push(variant);
                }
            }
        }
    }

    Some(Declaration {
        kind: DeclKind::Enum,
        name,
        signature,
        visibility: Visibility::Private,
        line,
        doc_comment,
        children: variants,
    })
}

fn extract_enum_member(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name").or_else(|| {
        // If it's a bare property_identifier, the node itself is the name
        if node.kind() == "property_identifier" {
            Some(node)
        } else {
            None
        }
    })?;
    let name = node_text(name_node, source).to_string();
    let line = node.start_position().row + 1;
    let text = node_text(node, source).trim().trim_end_matches(',');
    let signature = text.split_whitespace().collect::<Vec<_>>().join(" ");

    Some(Declaration {
        kind: DeclKind::Variant,
        name,
        signature,
        visibility: Visibility::Public,
        line,
        doc_comment: None,
        children: Vec::new(),
    })
}

fn extract_lexical_declaration(node: Node<'_>, source: &str) -> Vec<Declaration> {
    let mut declarations = Vec::new();

    // Determine if this is const, let, or var
    let mut decl_keyword = "const";
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            let text = node_text(child, source);
            if text == "const" || text == "let" || text == "var" {
                decl_keyword = if text == "const" { "const" } else { "let" };
                break;
            }
        }
    }

    // Only extract top-level const declarations
    if decl_keyword != "const" {
        return declarations;
    }

    for i in 0..node.child_count() {
        let Some(child) = node.child(i) else {
            continue;
        };
        if child.kind() == "variable_declarator" {
            if let Some(decl) = extract_variable_declarator(child, node, source) {
                declarations.push(decl);
            }
        }
    }

    declarations
}

fn extract_variable_declarator(
    node: Node<'_>,
    parent: Node<'_>,
    source: &str,
) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let doc_comment = extract_doc_comment(parent, source);
    let line = parent.start_position().row + 1;
    let signature = extract_signature(parent, source);

    Some(Declaration {
        kind: DeclKind::Constant,
        name,
        signature,
        visibility: Visibility::Private,
        line,
        doc_comment,
        children: Vec::new(),
    })
}
