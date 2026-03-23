use tree_sitter::Node;

use crate::model::Import;
use crate::model::declarations::{DeclKind, Declaration, Visibility};

use super::DeclExtractor;

pub struct CExtractor;

impl DeclExtractor for CExtractor {
    fn extract(&self, root: Node<'_>, source: &str) -> (Vec<Import>, Vec<Declaration>) {
        let mut imports = Vec::new();
        let mut declarations = Vec::new();

        for i in 0..root.child_count() {
            let Some(child) = root.child(i) else {
                continue;
            };
            match child.kind() {
                "preproc_include" => {
                    if let Some(import) = extract_include(child, source) {
                        imports.push(import);
                    }
                }
                "function_definition" => {
                    if let Some(decl) = extract_function_definition(child, source) {
                        declarations.push(decl);
                    }
                }
                "declaration" => {
                    if let Some(decl) = extract_declaration(child, source) {
                        declarations.push(decl);
                    }
                }
                "struct_specifier" => {
                    if let Some(decl) = extract_struct(child, source) {
                        declarations.push(decl);
                    }
                }
                "enum_specifier" => {
                    if let Some(decl) = extract_enum(child, source) {
                        declarations.push(decl);
                    }
                }
                "type_definition" => {
                    if let Some(decl) = extract_typedef(child, source) {
                        declarations.push(decl);
                    }
                }
                "preproc_def" => {
                    if let Some(decl) = extract_preproc_def(child, source) {
                        declarations.push(decl);
                    }
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

fn extract_visibility_from_text(node: Node<'_>, source: &str) -> Visibility {
    let text = node_text(node, source);
    if text.contains("static") {
        Visibility::Private
    } else {
        Visibility::Public
    }
}

fn extract_doc_comment(node: Node<'_>, source: &str) -> Option<String> {
    let mut comments = Vec::new();
    let mut prev = node.prev_sibling();

    while let Some(sibling) = prev {
        match sibling.kind() {
            "comment" => {
                let text = node_text(sibling, source);
                if text.starts_with("/**") {
                    let cleaned = text
                        .trim_start_matches("/**")
                        .trim_end_matches("*/")
                        .lines()
                        .map(|l| l.trim().trim_start_matches('*').trim())
                        .filter(|l| !l.is_empty())
                        .collect::<Vec<_>>()
                        .join(" ");
                    comments.push(cleaned);
                    break;
                } else if text.starts_with("/*") {
                    let cleaned = text
                        .trim_start_matches("/*")
                        .trim_end_matches("*/")
                        .trim()
                        .to_string();
                    comments.push(cleaned);
                    break;
                } else if let Some(doc) = text.strip_prefix("///") {
                    comments.push(doc.trim().to_string());
                } else if let Some(doc) = text.strip_prefix("//") {
                    comments.push(doc.trim().to_string());
                } else {
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
    let end = text
        .find('{')
        .or_else(|| text.find(';'))
        .unwrap_or(text.len());
    let sig = text[..end].trim();
    sig.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn extract_include(node: Node<'_>, source: &str) -> Option<Import> {
    let path_node = node.child_by_field_name("path")?;
    let text = node_text(path_node, source).to_string();
    Some(Import { text })
}

/// Extract function name from a function_declarator by traversing nested declarators.
/// function_definition → declarator (function_declarator) → declarator (identifier)
fn extract_function_name<'a>(declarator: Node<'_>, source: &'a str) -> Option<&'a str> {
    // The declarator field of a function_declarator is the actual identifier
    if declarator.kind() == "function_declarator" {
        if let Some(inner) = declarator.child_by_field_name("declarator") {
            if inner.kind() == "identifier" {
                return Some(node_text(inner, source));
            }
            // Could be pointer_declarator wrapping the identifier
            return extract_function_name(inner, source);
        }
    }
    if declarator.kind() == "pointer_declarator" {
        if let Some(inner) = declarator.child_by_field_name("declarator") {
            return extract_function_name(inner, source);
        }
    }
    if declarator.kind() == "identifier" {
        return Some(node_text(declarator, source));
    }
    None
}

fn extract_function_definition(node: Node<'_>, source: &str) -> Option<Declaration> {
    let declarator = node.child_by_field_name("declarator")?;
    let name = extract_function_name(declarator, source)?.to_string();
    let visibility = extract_visibility_from_text(node, source);
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

fn extract_declaration(node: Node<'_>, source: &str) -> Option<Declaration> {
    // Check if this declaration contains a function_declarator (function prototype)
    // or is a variable/constant declaration
    let declarator = node.child_by_field_name("declarator")?;

    if has_function_declarator(declarator) {
        // Function prototype
        let name = extract_function_name_from_decl(declarator, source)?.to_string();
        let visibility = extract_visibility_from_text(node, source);
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
    } else {
        // Variable declaration
        let name = extract_var_name(declarator, source)?.to_string();
        let visibility = extract_visibility_from_text(node, source);
        let doc_comment = extract_doc_comment(node, source);
        let signature = extract_signature(node, source);
        let line = node.start_position().row + 1;

        Some(Declaration {
            kind: DeclKind::Static,
            name,
            signature,
            visibility,
            line,
            doc_comment,
            children: Vec::new(),
        })
    }
}

fn has_function_declarator(node: Node<'_>) -> bool {
    if node.kind() == "function_declarator" {
        return true;
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if has_function_declarator(child) {
                return true;
            }
        }
    }
    false
}

fn extract_function_name_from_decl<'a>(node: Node<'_>, source: &'a str) -> Option<&'a str> {
    if node.kind() == "function_declarator" {
        return extract_function_name(node, source);
    }
    // Look for a function_declarator child
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if let Some(name) = extract_function_name_from_decl(child, source) {
                return Some(name);
            }
        }
    }
    None
}

fn extract_var_name<'a>(node: Node<'_>, source: &'a str) -> Option<&'a str> {
    // init_declarator has field "declarator" which is the identifier
    if node.kind() == "init_declarator" {
        if let Some(inner) = node.child_by_field_name("declarator") {
            return extract_var_name(inner, source);
        }
    }
    if node.kind() == "identifier" {
        return Some(node_text(node, source));
    }
    if node.kind() == "pointer_declarator" {
        if let Some(inner) = node.child_by_field_name("declarator") {
            return extract_var_name(inner, source);
        }
    }
    // Try the name field
    if let Some(name_node) = node.child_by_field_name("name") {
        return Some(node_text(name_node, source));
    }
    None
}

fn extract_struct(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let doc_comment = extract_doc_comment(node, source);
    let signature = extract_signature(node, source);
    let line = node.start_position().row + 1;

    let mut fields = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        for i in 0..body.child_count() {
            if let Some(child) = body.child(i) {
                if child.kind() == "field_declaration" {
                    if let Some(field) = extract_struct_field(child, source) {
                        fields.push(field);
                    }
                }
            }
        }
    }

    Some(Declaration {
        kind: DeclKind::Struct,
        name,
        signature,
        visibility: Visibility::Public,
        line,
        doc_comment,
        children: fields,
    })
}

fn extract_struct_field(node: Node<'_>, source: &str) -> Option<Declaration> {
    let declarator = node.child_by_field_name("declarator")?;
    let name = extract_var_name(declarator, source)?.to_string();
    let line = node.start_position().row + 1;

    let type_text = node
        .child_by_field_name("type")
        .map(|t| node_text(t, source))
        .unwrap_or("");

    let signature = format!("{} {}", type_text, name);
    let signature = signature.split_whitespace().collect::<Vec<_>>().join(" ");

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

fn extract_enum(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let doc_comment = extract_doc_comment(node, source);
    let signature = extract_signature(node, source);
    let line = node.start_position().row + 1;

    let mut variants = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        for i in 0..body.child_count() {
            if let Some(child) = body.child(i) {
                if child.kind() == "enumerator" {
                    if let Some(variant) = extract_enumerator(child, source) {
                        variants.push(variant);
                    }
                }
            }
        }
    }

    Some(Declaration {
        kind: DeclKind::Enum,
        name,
        signature,
        visibility: Visibility::Public,
        line,
        doc_comment,
        children: variants,
    })
}

fn extract_enumerator(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
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

fn extract_typedef(node: Node<'_>, source: &str) -> Option<Declaration> {
    let doc_comment = extract_doc_comment(node, source);
    let line = node.start_position().row + 1;
    let signature = extract_signature(node, source);

    // The typedef name is typically the last identifier-like declarator child.
    // Walk children from the end to find the declarator with the name.
    let declarator = node.child_by_field_name("declarator")?;
    let name = extract_var_name(declarator, source)
        .unwrap_or_else(|| node_text(declarator, source))
        .to_string();

    Some(Declaration {
        kind: DeclKind::TypeAlias,
        name,
        signature,
        visibility: Visibility::Public,
        line,
        doc_comment,
        children: Vec::new(),
    })
}

fn extract_preproc_def(node: Node<'_>, source: &str) -> Option<Declaration> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, source).to_string();
    let line = node.start_position().row + 1;
    let doc_comment = extract_doc_comment(node, source);
    let signature = node_text(node, source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    Some(Declaration {
        kind: DeclKind::Constant,
        name,
        signature,
        visibility: Visibility::Public,
        line,
        doc_comment,
        children: Vec::new(),
    })
}
